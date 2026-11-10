//! Full VSC station equations with explicit source or physical coordinates.
//! Includes equation/derivative diagnostics and experimental OPF;
//! problem and numerical preparation are owned by PowerIO.
use super::*;
mod coordinates;
mod derivatives;
mod opf;
mod pf;
pub use derivatives::{DerivativeCheck, DerivativeReport};
pub use opf::{
    AcDcOpfManifest, AcDcOpfProfile, AcDcOpfResult, AcDcOpfStart, AcDcScalar, PreparedAcDcOpf,
};
pub use pf::{AcDcPfManifest, AcDcPfResult, AcDcPfStart, PreparedAcDcPf};
use powerio::BalancedNetwork;
pub use powerio::{AcDcAngleReferencePolicy, AcDcCoordinatePolicy, AcDcCurrentLimitPolicy};
pub use powerio_matrix::{AcDcAngleReference, AcDcBranchPolicy};
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

pub use powerio_matrix::AcDcStationPolicy as StationPolicy;

#[derive(Debug, serde::Serialize)]
pub struct EquationCheck {
    pub name: String,
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
    pub violation: f64,
}

/// PowerModels parser bus-role repair, kept separate from source declarations.
#[derive(Clone, Debug, serde::Serialize)]
pub struct BusTypePolicy {
    pub bus_id: usize,
    pub declared: powerio_tx::BusType,
    pub effective: powerio_tx::BusType,
}

#[derive(Debug, serde::Serialize)]
pub struct EquationReport {
    pub profile: &'static str,
    pub current_limit_policy: AcDcCurrentLimitPolicy,
    pub coordinate_policy: AcDcCoordinatePolicy,
    pub dc_branch_policy: Vec<AcDcBranchPolicy>,
    pub mode: EquationMode,
    pub objective: f64,
    pub max_violation: f64,
    pub equations: Vec<EquationCheck>,
    pub bounds: Vec<EquationCheck>,
    pub station_policy: Vec<StationPolicy>,
    pub bus_type_policy: Vec<BusTypePolicy>,
    pub angle_reference_policy: AcDcAngleReferencePolicy,
    pub angle_references: Vec<AcDcAngleReference>,
}

/// Recorded coordinate transformation, not an edit to source equipment.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AcDcAngleShift {
    pub anchor_bus: usize,
    pub shift_radians: f64,
    pub variables: Vec<String>,
}

/// Native expression model with identity-named pu variables (angles in radians).
/// Its expressions are also suitable for later native NLP integration. Building
/// this model never modifies the source owner or enables legacy AC-only solves.
pub struct ReferenceEquations {
    model: CanonicalAcOpfModel,
    mode: EquationMode,
    profile: AcDcOpfProfile,
    policy: Vec<StationPolicy>,
    dc_branch_policy: Vec<AcDcBranchPolicy>,
    bus_type_policy: Vec<BusTypePolicy>,
    angle_reference_policy: AcDcAngleReferencePolicy,
    angle_references: Vec<AcDcAngleReference>,
    station_buses: BTreeMap<String, powerio::BusId>,
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
    voltage_ratios: [f64; 2],
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
            voltage_ratios[0],
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
            subtract(
                if voltage_ratios[1] == 1.0 {
                    vm[0].clone()
                } else {
                    scaled(voltage_ratios[1], vm[0].clone())
                },
                vm[1].clone(),
            ),
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
        let instance = powerio::AcDcOpfInstance::from_network(
            net.clone(),
            powerio::AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
        )
        .map_err(|e| e.to_string())?;
        Self::from_instance(&instance, mode)
    }
    pub fn from_instance(
        instance: &powerio::AcDcOpfInstance,
        mode: EquationMode,
    ) -> Result<Self, String> {
        Self::compile(instance, mode, None)
    }
    /// PF semantics are selected by canonical PF intent, never OPF flags.
    pub fn from_pf_instance(instance: &powerio::AcDcPfInstance) -> Result<Self, String> {
        let mut network = instance.network().clone();
        // PF has no objective or cost auxiliaries. Retain costs in the owner,
        // removing them only from this private equation-assembly projection.
        for g in network.generators_mut() {
            g.cost = Some(powerio::GenCost::new(2, 0., 0., vec![0.]));
        }
        let adapter = powerio::AcDcOpfInstance::from_network(
            network,
            AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
        )
        .map_err(|e| e.to_string())?;
        Self::compile(&adapter, EquationMode::Pf, Some(instance.profile()))
    }
    fn compile(
        instance: &powerio::AcDcOpfInstance,
        mode: EquationMode,
        pf_profile: Option<powerio::AcDcPfProfile>,
    ) -> Result<Self, String> {
        match instance.profile() {
            AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1 => {}
            AcDcOpfProfile::DeclaredCurrentAcpVscV1
            | AcDcOpfProfile::PhysicalCoordinatesAcpVscV1
            | AcDcOpfProfile::FixedModePhysicalAcpVscV1
                if mode == EquationMode::Opf => {}
            AcDcOpfProfile::DeclaredCurrentAcpVscV1
            | AcDcOpfProfile::PhysicalCoordinatesAcpVscV1
            | AcDcOpfProfile::FixedModePhysicalAcpVscV1 => return Err(
                "Selected profile qualifies OPF only; PF controls/operating limits are unqualified"
                    .into(),
            ),
            _ => return Err("Unsupported coupled OPF profile".into()),
        }
        let net = instance.network();
        validate_supported_network(net)?;
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
                && !(s.dc_control == DcConverterControl::DcPowerDroop
                    && pf_profile.is_some_and(|profile| profile.allows_dc_power_droop()))
            {
                return Err(format!(
                    "{}: PF droop controls are not qualified",
                    s.component
                ));
            }
        }
        let generator_buses: std::collections::BTreeSet<_> = net
            .generators()
            .iter()
            .filter(|g| g.in_service)
            .map(|g| g.bus)
            .collect();
        // The pinned PowerModels parser repairs PQ/PV roles by active-generator
        // presence. Reference reassignment needs separate qualification.
        if net
            .buses()
            .iter()
            .any(|b| b.kind == powerio_tx::BusType::Ref && !generator_buses.contains(&b.id))
        {
            return Err("Reference bus without active generation needs an unqualified reference reassignment".into());
        }
        let effective_kind = |b: &powerio_tx::Bus| match b.kind {
            powerio_tx::BusType::Pq if generator_buses.contains(&b.id) => powerio_tx::BusType::Pv,
            powerio_tx::BusType::Pv if !generator_buses.contains(&b.id) => powerio_tx::BusType::Pq,
            kind => kind,
        };
        let bus_type_policy = net
            .buses()
            .iter()
            .filter(|b| b.kind != effective_kind(b))
            .map(|b| BusTypePolicy {
                bus_id: b.id.0,
                declared: b.kind,
                effective: effective_kind(b),
            })
            .collect();
        let coupled =
            powerio_matrix::build_ac_dc_opf_preparation(instance).map_err(|e| e.to_string())?;
        if mode == EquationMode::Pf && coupled.angle_references.iter().any(|r| !r.declared) {
            return Err(
                "Numerical OPF angle references do not qualify PF reference/dispatch controls"
                    .into(),
            );
        }
        let angle_references = coupled.angle_references.clone();
        let dc_branch_policy = coupled
            .dc_branches
            .iter()
            .map(|b| b.policy.clone())
            .collect();
        let mut policy = Vec::new();
        let model = compile_ac_opf_preparation_with(
            net,
            &powerio_prob::ActiveConstraints::default(),
            instance.initial_point().is_some(),
            coupled.ac,
            |prep, cols, rows, p| {
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
                        if matches!(
                            effective_kind(b),
                            powerio_tx::BusType::Ref | powerio_tx::BusType::Pv
                        ) {
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
                        if effective_kind(b) != powerio_tx::BusType::Ref {
                            setpoint(
                                p,
                                format!("pf/pg[{identity}]"),
                                variable(cols.pg[j]),
                                g.pg / base,
                            );
                            if effective_kind(b) == powerio_tx::BusType::Pq {
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
                if pf_profile.is_some() {
                    pf::generator_sharing(net, prep, cols, p)?;
                }
                let mut dcvm = Vec::new();
                let mut kcl = Vec::new();
                for bus in &coupled.dc_buses {
                    let b = &bus.source;
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
                    kcl.push(vec![constant(bus.withdrawal_pu)]);
                }
                let index = |id: &powerio::ComponentId| {
                    owner
                        .buses
                        .iter()
                        .position(|b| b.component == *id)
                        .ok_or_else(|| format!("Unmapped DC bus {id}"))
                };
                for branch in &coupled.dc_branches {
                    let b = &branch.source;
                    let (f, t) = (index(&b.from)?, index(&b.to)?);
                    if b.resistance_pu <= 0.0 {
                        return Err(format!(
                            "{}: zero-resistance DC branches are not qualified",
                            b.component
                        ));
                    }
                    let limit = if mode == EquationMode::Opf {
                        branch.active_power_limit_pu
                    } else {
                        INF
                    };
                    let vf = dcvm[f].clone();
                    let vt = if branch.policy.coordinate_policy
                        == AcDcCoordinatePolicy::PhysicalLocalBases
                    {
                        scaled(branch.policy.to_voltage_scale, dcvm[t].clone())
                    } else {
                        dcvm[t].clone()
                    };
                    for (label, from, v_from, v_to) in
                        [("pf", f, vf.clone(), vt.clone()), ("pt", t, vt, vf)]
                    {
                        let name = format!("dc_branch/{}/{label}", b.component);
                        let v = col(p, name.clone(), -limit, limit, 0.0);
                        let expected = scaled(
                            branch.conductance_pu,
                            multiply(v_from.clone(), subtract(v_from, v_to)),
                        );
                        eq(p, name, subtract(v.clone(), expected));
                        kcl[from].push(v);
                    }
                }
                let shared_voltage =
                    pf_profile.is_some_and(|profile| profile.shares_voltage_reactive_increments());
                let mut voltage_buses: std::collections::BTreeSet<_> = net
                    .buses()
                    .iter()
                    .filter(|b| {
                        matches!(
                            effective_kind(b),
                            powerio_tx::BusType::Ref | powerio_tx::BusType::Pv
                        )
                    })
                    .map(|b| b.id)
                    .collect();
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
                    let station = coupled
                        .stations
                        .iter()
                        .find(|v| v.source.component == s.component)
                        .ok_or("Station preparation missing")?;
                    let loss = station.policy.loss_coefficients_pu;
                    policy.push(station.policy.clone());
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
                            let quantity = coordinates::station_quantity(field)
                                .ok_or("Unknown station coordinate")?;
                            let bounds =
                                station.bounds(quantity).ok_or("Station bounds missing")?;
                            (bounds.min, bounds.max)
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
                        [
                            s.transformer_tap_ratio,
                            station.policy.transformer_bypass_voltage_ratio,
                        ],
                        [variable(cols.vm[aci]), val("vmfilt")],
                        [variable(cols.va[aci]), val("vafilt")],
                        [val("pgrid"), val("qgrid"), val("ptf_to"), val("qtf_to")],
                    )?;
                    series(
                        p,
                        &format!("{name}/reactor"),
                        &s.reactor,
                        [1.0, 1.0],
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
                            DcConverterControl::DcPowerDroop
                                if pf_profile
                                    .is_some_and(|profile| profile.allows_dc_power_droop()) =>
                            {
                                setpoint(
                                    p,
                                    format!("{name}/dc_droop_setpoint"),
                                    add(
                                        val("pdc"),
                                        scaled(
                                            s.settings.droop.recip(),
                                            subtract(
                                                dcvm[dci].clone(),
                                                constant(s.settings.dc_voltage_target_pu),
                                            ),
                                        ),
                                    ),
                                    s.settings.dc_withdrawal_target_mw / base,
                                )
                            }
                            _ => return Err("Unsupported PF DC control".into()),
                        }
                        match s.ac_control {
                            AcConverterControl::ReactivePowerAtPcc => setpoint(
                                p,
                                format!("{name}/q_setpoint"),
                                val("qgrid"),
                                s.settings.pcc_withdrawal_mvar / base,
                            ),
                            AcConverterControl::VoltageAtPcc => {
                                if !shared_voltage || voltage_buses.insert(s.ac_bus) {
                                    setpoint(
                                        p,
                                        format!("{name}/vac_setpoint"),
                                        variable(cols.vm[aci]),
                                        pcc.vm,
                                    );
                                }
                            }
                            _ => return Err("Unsupported PF AC control".into()),
                        }
                    }
                }
                if shared_voltage {
                    pf::voltage_sharing(net, p)?;
                }
                for (i, terms) in kcl.into_iter().enumerate() {
                    eq(
                        p,
                        format!("dc_bus/{}/balance", owner.buses[i].component),
                        sum(terms),
                    );
                }
                Ok(())
            },
        )?;
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
            profile: instance.profile(),
            policy,
            dc_branch_policy,
            bus_type_policy,
            angle_reference_policy: instance.angle_reference_policy(),
            angle_references,
            station_buses: owner
                .stations
                .iter()
                .filter(|s| s.in_service)
                .map(|s| (s.component.to_string(), s.ac_bus))
                .collect(),
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

    /// Explicitly align only added numerical references. All bus, filter and
    /// valve angles in that AC island receive the same shift. Supplied starts
    /// are never silently changed; callers opt into this operation and retain
    /// its shift record. Reject rotations outside any original angle bound.
    pub fn align_angle_references(
        &self,
        point: &BTreeMap<String, f64>,
    ) -> Result<(BTreeMap<String, f64>, Vec<AcDcAngleShift>), String> {
        self.point_values(point)?;
        let mut aligned = point.clone();
        let mut shifts = Vec::new();
        for reference in self.angle_references.iter().filter(|r| !r.declared) {
            let shift = -point[&format!("va[{}]", reference.bus.0)];
            let mut variables: Vec<_> = reference
                .island_buses
                .iter()
                .map(|b| format!("va[{}]", b.0))
                .collect();
            for (station, bus) in &self.station_buses {
                if reference.island_buses.contains(bus) {
                    variables.extend([
                        format!("station/{station}/vafilt"),
                        format!("station/{station}/vaconv"),
                    ]);
                }
            }
            variables.sort();
            for name in &variables {
                let value = aligned.get_mut(name).ok_or("Angle coordinate missing")?;
                *value += shift;
                let p = &self.model.problem;
                let column = p
                    .var_names
                    .iter()
                    .position(|v| v == name)
                    .ok_or("Angle column missing")?;
                if !value.is_finite() || *value < p.x_l[column] || *value > p.x_u[column] {
                    return Err(format!("Angle alignment exceeds original bounds at {name}"));
                }
            }
            shifts.push(AcDcAngleShift {
                anchor_bus: reference.bus.0,
                shift_radians: shift,
                variables,
            });
        }
        Ok((aligned, shifts))
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
            profile: match self.profile {
                AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1 => {
                    "powermodelsacdc-9ada4082-acp-source-coordinates-inverter-loss"
                }
                AcDcOpfProfile::DeclaredCurrentAcpVscV1 => {
                    "acp-vsc-source-coordinates-inverter-loss-declared-current-v1"
                }
                AcDcOpfProfile::PhysicalCoordinatesAcpVscV1 => {
                    "acp-vsc-physical-local-bases-inverter-loss-declared-current-v1"
                }
                AcDcOpfProfile::FixedModePhysicalAcpVscV1 => {
                    "acp-vsc-physical-local-bases-fixed-mode-loss-declared-current-v1"
                }
                _ => unreachable!("profile checked at construction"),
            },
            current_limit_policy: self.profile.current_limit_policy(),
            coordinate_policy: self.profile.coordinate_policy(),
            dc_branch_policy: self.dc_branch_policy.clone(),
            mode: self.mode,
            objective,
            max_violation,
            equations,
            bounds,
            station_policy: self.policy.clone(),
            bus_type_policy: self.bus_type_policy.clone(),
            angle_reference_policy: self.angle_reference_policy,
            angle_references: self.angle_references.clone(),
        })
    }
}
