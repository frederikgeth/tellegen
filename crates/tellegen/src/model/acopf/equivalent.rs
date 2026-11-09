//! Full VSC station equations in the pinned PowerModelsACDC 0.10.1 coordinate
//! convention. This is an equation qualification API, not a coupled solve API.
use super::*;
mod derivatives;
pub use derivatives::{DerivativeCheck, DerivativeReport};
use powerio::BalancedNetwork;
use powerio_tx::acdc::{
    AcConverterControl, ConverterSeries, ConverterTechnology, DcConverterControl,
};
use std::collections::BTreeMap;

/// PF adds setpoint equations and omits OPF operating limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EquationMode {
    Pf,
    Opf,
}

/// Declared versus effective parameters for the explicitly selected reference
/// policy. Station impedances stay in source pu coordinates even when the PCC
/// base differs; loss scaling uses the station AC base, never the PCC base.
#[derive(Clone, Debug, serde::Serialize)]
pub struct StationPolicy {
    pub component: String,
    pub converter_base_kv: f64,
    pub pcc_base_kv: f64,
    pub declared_current_limit_pu: f64,
    pub effective_current_limit_pu: f64,
    pub loss_coefficients_pu: [f64; 3],
    pub declared_rectifier_quadratic_ohm: f64,
    pub declared_inverter_quadratic_ohm: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct EquationCheck {
    pub name: String,
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
    pub violation: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct EquationReport {
    pub profile: &'static str,
    pub mode: EquationMode,
    pub objective: f64,
    pub max_violation: f64,
    pub equations: Vec<EquationCheck>,
    pub bounds: Vec<EquationCheck>,
    pub station_policy: Vec<StationPolicy>,
}

/// Native expression model with identity-named pu variables (angles in radians).
/// Its expressions are also suitable for later native NLP integration. Building
/// this model never modifies the source owner or enables legacy AC-only solves.
pub struct ReferenceEquations {
    model: CanonicalAcOpfModel,
    mode: EquationMode,
    policy: Vec<StationPolicy>,
}

fn col(p: &mut NlProblemParts, name: String, lo: f64, hi: f64, start: f64) -> Expr {
    variable(super::hybrid::column(p, name, lo, hi, start))
}
fn eq(p: &mut NlProblemParts, name: String, expression: Expr) {
    super::hybrid::row(p, name, expression, 0.0, 0.0);
}
fn setpoint(p: &mut NlProblemParts, name: String, expression: Expr, target: f64) {
    eq(p, name, subtract(expression, constant(target)));
}

fn series(
    p: &mut NlProblemParts,
    name: &str,
    z: &ConverterSeries,
    tap: f64,
    vm: [Expr; 2],
    va: [Expr; 2],
    flow: [Expr; 4],
) -> Result<(), String> {
    let [pf, qf, pt, qt] = flow;
    if z.enabled {
        let norm = z.resistance_pu.powi(2) + z.reactance_pu.powi(2);
        if norm <= 0.0 || !norm.is_finite() {
            return Err(format!("{name}: invalid enabled series impedance"));
        }
        let squares = [square(vm[0].clone()), square(vm[1].clone())];
        let f = polar_branch_flow(
            vm,
            va,
            [z.resistance_pu / norm, -z.reactance_pu / norm],
            tap,
            0.0,
            [0.0; 4],
            squares,
        );
        for (suffix, actual, expected) in [
            ("pf", pf, f.p_from),
            ("qf", qf, f.q_from),
            ("pt", pt, f.p_to),
            ("qt", qt, f.q_to),
        ] {
            eq(p, format!("{name}/{suffix}"), subtract(actual, expected));
        }
    } else {
        eq(p, format!("{name}/p_bypass"), add(pf, pt));
        eq(p, format!("{name}/q_bypass"), add(qf, qt));
        eq(
            p,
            format!("{name}/vm_bypass"),
            subtract(vm[0].clone(), vm[1].clone()),
        );
        eq(
            p,
            format!("{name}/va_bypass"),
            subtract(va[0].clone(), va[1].clone()),
        );
    }
    Ok(())
}

impl ReferenceEquations {
    /// Select the pinned reference policy: source station pu coordinates,
    /// inverter loss coefficient in both directions, and reference Imax repair.
    /// Only VSC and constant P/Q or voltage PF controls are qualified here.
    pub fn powermodels_acdc_0101(
        net: &BalancedNetwork,
        mode: EquationMode,
    ) -> Result<Self, String> {
        let owner = net
            .ac_dc_equivalent()
            .ok_or("AC/DC equivalent owner missing")?;
        owner.validate(net).map_err(|e| e.to_string())?;
        if net.is_normalized() || owner.base_mva != net.base_mva() {
            return Err("Reference equations require unnormalized AC data on the declared equipment MVA base".into());
        }
        if net.hvdc().iter().any(|h| h.in_service) {
            return Err("Reference equations refuse overlapping aggregate HVDC".into());
        }
        if let Some(d) = net.detailed_connectivity().as_ref() {
            if !d.voltage_source_converters.is_empty()
                || !d.line_commutated_converters.is_empty()
                || !d.dc_lines.is_empty()
            {
                return Err(
                    "Reference equations refuse mixed physical and equivalent DC equipment".into(),
                );
            }
        }
        for s in owner.stations.iter().filter(|s| s.in_service) {
            if s.technology != ConverterTechnology::Vsc {
                return Err(format!("{}: only VSC is qualified", s.component));
            }
            if mode == EquationMode::Pf
                && !matches!(
                    s.dc_control,
                    DcConverterControl::ActivePowerAtPcc | DcConverterControl::DcVoltage
                )
            {
                return Err(format!(
                    "{}: PF droop controls are not qualified",
                    s.component
                ));
            }
        }
        // Deliberate internal AC projection: every active equivalent component
        // is assembled below. Ordinary PowerIO instance guards remain intact.
        let mut ac = net.clone();
        ac.set_ac_dc_equivalent(None).map_err(|e| e.to_string())?;
        let instance = AcOpfInstance::from_network(ac).map_err(|e| e.to_string())?;
        let mut policy = Vec::new();
        let model = compile_ac_opf_model_with(&instance, |prep, cols, rows, p| {
            let base = owner.base_mva;
            let vm2 = cols
                .vm
                .iter()
                .map(|&v| square(variable(v)))
                .collect::<Vec<_>>();
            // AC terminal powers are observable expression definitions, so the
            // oracle checks both endpoints even though AC balance eliminates them.
            for j in 0..prep.n_branches() {
                let f = branch_flow(prep, cols, &vm2, j);
                for (label, expr) in [
                    ("pf", f.p_from),
                    ("qf", f.q_from),
                    ("pt", f.p_to),
                    ("qt", f.q_to),
                ] {
                    let name = format!("ac_branch/{}/{label}", prep.branches.identities[j]);
                    let v = col(p, name.clone(), -INF, INF, 0.0);
                    eq(p, name, subtract(v, expr));
                }
            }
            if mode == EquationMode::Pf {
                p.objective = constant(0.0);
                p.obj_constant = 0.0;
                // PF is unbounded in the reference. Retain only AC equalities;
                // OPF inequalities must not be reported as PF requirements.
                p.x_l.fill(-INF);
                p.x_u.fill(INF);
                // Remove by row role, including an OPF angle interval whose
                // endpoints happen to coincide (it is still not a PF control).
                for &i in rows
                    .angle_difference
                    .iter()
                    .chain(&rows.thermal_from)
                    .chain(&rows.thermal_to)
                    .flatten()
                    .chain(rows.piecewise_segments.iter().flatten())
                {
                    p.g_l[i] = -INF;
                    p.g_u[i] = INF;
                }
                for (i, id) in prep.bus_ids.iter().enumerate() {
                    let b = net
                        .buses()
                        .iter()
                        .find(|b| b.id == *id)
                        .ok_or("AC bus map missing")?;
                    if matches!(b.kind, powerio_tx::BusType::Ref | powerio_tx::BusType::Pv) {
                        setpoint(p, format!("pf/vm[{}]", id.0), variable(cols.vm[i]), b.vm);
                    }
                }
                for (j, identity) in prep.generators.identities.iter().enumerate() {
                    let g = net
                        .generators()
                        .iter()
                        .find(|g| g.uid.as_deref() == Some(identity.as_str()))
                        .ok_or("Generator UID map missing")?;
                    let b = net
                        .buses()
                        .iter()
                        .find(|b| b.id == g.bus)
                        .ok_or("Generator bus missing")?;
                    if b.kind != powerio_tx::BusType::Ref {
                        setpoint(
                            p,
                            format!("pf/pg[{identity}]"),
                            variable(cols.pg[j]),
                            g.pg / base,
                        );
                        if b.kind == powerio_tx::BusType::Pq {
                            setpoint(
                                p,
                                format!("pf/qg[{identity}]"),
                                variable(cols.qg[j]),
                                g.qg / base,
                            );
                        }
                    }
                }
            }
            let mut dcvm = Vec::new();
            let mut kcl = Vec::new();
            for b in &owner.buses {
                let (lo, hi) = if mode == EquationMode::Opf {
                    (b.voltage_limits_pu.min, b.voltage_limits_pu.max)
                } else {
                    (-INF, INF)
                };
                dcvm.push(col(
                    p,
                    format!("dc_bus/{}/vm", b.component),
                    lo,
                    hi,
                    b.initial_voltage_pu,
                ));
                kcl.push(vec![constant(b.withdrawal_mw / base)]);
            }
            let index = |id: &powerio::ComponentId| {
                owner
                    .buses
                    .iter()
                    .position(|b| b.component == *id)
                    .ok_or_else(|| format!("Unmapped DC bus {id}"))
            };
            for b in owner.branches.iter().filter(|b| b.in_service) {
                let (f, t) = (index(&b.from)?, index(&b.to)?);
                if b.resistance_pu <= 0.0 {
                    return Err(format!(
                        "{}: zero-resistance DC branches are not qualified",
                        b.component
                    ));
                }
                let limit = if mode == EquationMode::Opf {
                    b.ratings_mw[0] / base
                } else {
                    INF
                };
                for (label, from, to) in [("pf", f, t), ("pt", t, f)] {
                    let name = format!("dc_branch/{}/{label}", b.component);
                    let v = col(p, name.clone(), -limit, limit, 0.0);
                    let expected = scaled(
                        f64::from(owner.pole_count) / b.resistance_pu,
                        multiply(
                            dcvm[from].clone(),
                            subtract(dcvm[from].clone(), dcvm[to].clone()),
                        ),
                    );
                    eq(p, name, subtract(v.clone(), expected));
                    kcl[from].push(v);
                }
            }
            for s in owner.stations.iter().filter(|s| s.in_service) {
                let name = format!("station/{}", s.component);
                let aci = prep
                    .bus_ids
                    .iter()
                    .position(|b| *b == s.ac_bus)
                    .ok_or("Station PCC omitted by AC preparation")?;
                let dci = index(&s.dc_bus)?;
                let pcc = net
                    .buses()
                    .iter()
                    .find(|b| b.id == s.ac_bus)
                    .ok_or("PCC missing")?;
                let pr = s.valve_active_mw.min.abs().max(s.valve_active_mw.max.abs()) / base;
                let qr = s
                    .valve_reactive_mvar
                    .min
                    .abs()
                    .max(s.valve_reactive_mvar.max.abs())
                    / base;
                let imax = s.valve_current_limit_pu.max(pr.hypot(qr));
                let loss = [
                    s.losses.constant_mw / base,
                    s.losses.linear_kv / (3.0_f64.sqrt() * s.ac_base_kv),
                    s.losses.inverter_quadratic_ohm * base / (3.0 * s.ac_base_kv.powi(2)),
                ];
                policy.push(StationPolicy {
                    component: s.component.to_string(),
                    converter_base_kv: s.ac_base_kv,
                    pcc_base_kv: pcc.base_kv,
                    declared_current_limit_pu: s.valve_current_limit_pu,
                    effective_current_limit_pu: imax,
                    loss_coefficients_pu: loss,
                    declared_rectifier_quadratic_ohm: s.losses.rectifier_quadratic_ohm,
                    declared_inverter_quadratic_ohm: s.losses.inverter_quadratic_ohm,
                });
                let mut v = BTreeMap::new();
                for field in [
                    "pgrid", "qgrid", "ptf_to", "qtf_to", "ppr_fr", "qpr_fr", "pconv", "qconv",
                    "pdc", "iconv", "vmfilt", "vafilt", "vmconv", "vaconv",
                ] {
                    let (lo, hi) = if mode == EquationMode::Pf {
                        if field == "iconv" {
                            (0.0, INF)
                        } else {
                            (-INF, INF)
                        }
                    } else {
                        match field {
                            "pconv" => (s.valve_active_mw.min / base, s.valve_active_mw.max / base),
                            "qconv" => (
                                s.valve_reactive_mvar.min / base,
                                s.valve_reactive_mvar.max / base,
                            ),
                            "vmconv" => (s.valve_voltage_pu.min, s.valve_voltage_pu.max),
                            "iconv" => (0.0, imax),
                            "pgrid" | "ptf_to" | "ppr_fr" => (-2.0 * pr, 2.0 * pr),
                            "qgrid" | "qtf_to" | "qpr_fr" => (-2.0 * qr, 2.0 * qr),
                            "pdc" => (-1.2 * pr, 1.2 * pr),
                            "vmfilt" => {
                                (s.valve_voltage_pu.min / 1.2, s.valve_voltage_pu.max * 1.2)
                            }
                            "vafilt" | "vaconv" => (-std::f64::consts::TAU, std::f64::consts::TAU),
                            _ => unreachable!(),
                        }
                    };
                    v.insert(
                        field,
                        col(
                            p,
                            format!("{name}/{field}"),
                            lo,
                            hi,
                            if field.starts_with("vm") { 1.0 } else { 0.0 },
                        ),
                    );
                }
                let val = |key: &str| v[key].clone();
                series(
                    p,
                    &format!("{name}/transformer"),
                    &s.transformer,
                    s.transformer_tap_ratio,
                    [variable(cols.vm[aci]), val("vmfilt")],
                    [variable(cols.va[aci]), val("vafilt")],
                    [val("pgrid"), val("qgrid"), val("ptf_to"), val("qtf_to")],
                )?;
                series(
                    p,
                    &format!("{name}/reactor"),
                    &s.reactor,
                    1.0,
                    [val("vmfilt"), val("vmconv")],
                    [val("vafilt"), val("vaconv")],
                    [
                        val("ppr_fr"),
                        val("qpr_fr"),
                        scaled(-1.0, val("pconv")),
                        scaled(-1.0, val("qconv")),
                    ],
                )?;
                eq(
                    p,
                    format!("{name}/filter_p"),
                    add(val("ptf_to"), val("ppr_fr")),
                );
                eq(
                    p,
                    format!("{name}/filter_q"),
                    subtract(
                        add(val("qtf_to"), val("qpr_fr")),
                        scaled(
                            if s.filter_enabled {
                                s.filter_susceptance_pu
                            } else {
                                0.0
                            },
                            square(val("vmfilt")),
                        ),
                    ),
                );
                eq(
                    p,
                    format!("{name}/current"),
                    subtract(
                        add(square(val("pconv")), square(val("qconv"))),
                        multiply(square(val("vmconv")), square(val("iconv"))),
                    ),
                );
                eq(
                    p,
                    format!("{name}/loss"),
                    subtract(
                        add(val("pconv"), val("pdc")),
                        sum(vec![
                            constant(loss[0]),
                            scaled(loss[1], val("iconv")),
                            scaled(loss[2], square(val("iconv"))),
                        ]),
                    ),
                );
                p.constraints[rows.p_balance[aci]] =
                    subtract(p.constraints[rows.p_balance[aci]].clone(), val("pgrid"));
                p.constraints[rows.q_balance[aci]] =
                    subtract(p.constraints[rows.q_balance[aci]].clone(), val("qgrid"));
                kcl[dci].push(val("pdc"));
                if mode == EquationMode::Pf {
                    match s.dc_control {
                        DcConverterControl::ActivePowerAtPcc => setpoint(
                            p,
                            format!("{name}/p_setpoint"),
                            val("pgrid"),
                            s.settings.pcc_withdrawal_mw / base,
                        ),
                        // Reference PF selects the retained converter Vdcset.
                        DcConverterControl::DcVoltage => setpoint(
                            p,
                            format!("{name}/vdc_setpoint"),
                            dcvm[dci].clone(),
                            s.settings.dc_voltage_target_pu,
                        ),
                        _ => return Err("Unsupported PF DC control".into()),
                    }
                    match s.ac_control {
                        AcConverterControl::ReactivePowerAtPcc => setpoint(
                            p,
                            format!("{name}/q_setpoint"),
                            val("qgrid"),
                            s.settings.pcc_withdrawal_mvar / base,
                        ),
                        AcConverterControl::VoltageAtPcc => setpoint(
                            p,
                            format!("{name}/vac_setpoint"),
                            variable(cols.vm[aci]),
                            pcc.vm,
                        ),
                        _ => return Err("Unsupported PF AC control".into()),
                    }
                }
            }
            for (i, terms) in kcl.into_iter().enumerate() {
                eq(
                    p,
                    format!("dc_bus/{}/balance", owner.buses[i].component),
                    sum(terms),
                );
            }
            Ok(())
        })?;
        if model
            .problem
            .var_names
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != model.problem.n
        {
            return Err("Ambiguous native variable identities".into());
        }
        Ok(Self {
            model,
            mode,
            policy,
        })
    }

    pub fn variable_names(&self) -> &[String] {
        &self.model.problem.var_names
    }

    /// Require exactly one finite value for every native variable; missing or
    /// extra entries are errors, never silently filled with model starts.
    fn point_values(&self, point: &BTreeMap<String, f64>) -> Result<Vec<f64>, String> {
        if point.len() != self.variable_names().len() {
            return Err("Point has a different variable count".into());
        }
        self.variable_names()
            .iter()
            .map(|n| {
                point
                    .get(n)
                    .copied()
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| format!("Missing/nonfinite variable {n}"))
            })
            .collect()
    }

    /// Evaluate native equations and bounds at a complete finite operating point.
    pub fn evaluate(&self, point: &BTreeMap<String, f64>) -> Result<EquationReport, String> {
        let x = self.point_values(point)?;
        let problem = &self.model.problem;
        let mut nlp = self.model.tnlp()?;
        let mut g = vec![0.0; problem.m];
        let objective = nlp.eval_f(&x, true).ok_or("Objective evaluation failed")?;
        if !nlp.eval_g(&x, true, &mut g)
            || !objective.is_finite()
            || g.iter().any(|v| !v.is_finite())
        {
            return Err("Nonfinite native equation evaluation".into());
        }
        let check = |name: String, value: f64, lower: f64, upper: f64| EquationCheck {
            name,
            value,
            lower,
            upper,
            violation: (lower - value).max(value - upper).max(0.0),
        };
        let equations = g
            .into_iter()
            .enumerate()
            .filter(|(i, _)| problem.g_l[*i] > -INF || problem.g_u[*i] < INF)
            .map(|(i, v)| {
                check(
                    problem.con_names[i].clone(),
                    v,
                    problem.g_l[i],
                    problem.g_u[i],
                )
            })
            .collect::<Vec<_>>();
        let bounds = x
            .into_iter()
            .enumerate()
            .filter(|(i, _)| problem.x_l[*i] > -INF || problem.x_u[*i] < INF)
            .map(|(i, v)| {
                check(
                    problem.var_names[i].clone(),
                    v,
                    problem.x_l[i],
                    problem.x_u[i],
                )
            })
            .collect::<Vec<_>>();
        let max_violation = equations
            .iter()
            .chain(&bounds)
            .map(|v| v.violation)
            .fold(0.0, f64::max);
        Ok(EquationReport {
            profile: "powermodelsacdc-9ada4082-acp-source-coordinates-inverter-loss",
            mode: self.mode,
            objective,
            max_violation,
            equations,
            bounds,
            station_policy: self.policy.clone(),
        })
    }
}
