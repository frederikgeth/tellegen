//! Square native PF with explicit generator sharing and post-solve limits.
use super::*;
use powerio::{AcDcPfInstance, AcDcPfProfile, AcDcPfSolution};
use serde::{Deserialize, Serialize};
mod newton;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcDcPfStart {
    Declared,
    Flat,
    InstancePoint,
    Point {
        model_fingerprint: String,
        values: BTreeMap<String, f64>,
    },
}
#[derive(Debug, Serialize)]
pub struct AcDcPfManifest {
    pub schema: &'static str,
    pub profile: AcDcPfProfile,
    pub powerio_revision: &'static str,
    pub reference_revision: &'static str,
    pub model_fingerprint: String,
    pub variables: Vec<String>,
    pub equalities: Vec<String>,
    pub controls: Vec<String>,
    pub angle_references: Vec<AcDcAngleReference>,
    pub bus_type_policy: Vec<BusTypePolicy>,
    pub limit_policy: &'static str,
}
pub struct PreparedAcDcPf {
    instance: Arc<AcDcPfInstance>,
    adapter: powerio::AcDcOpfInstance,
    equations: ReferenceEquations,
    limits: ReferenceEquations,
    manifest: AcDcPfManifest,
    declared: BTreeMap<String, f64>,
}
#[derive(Debug, Serialize)]
pub struct AcDcPfResult {
    #[serde(skip)]
    pub solution: AcDcPfSolution,
    pub model_fingerprint: String,
    pub solver: &'static str,
    pub iterations: usize,
    /// Accepted steps retracted to the nonnegative valve-current equality.
    pub current_retractions: usize,
    pub residual_tolerance: f64,
    pub initial_point: BTreeMap<String, f64>,
    pub point: BTreeMap<String, f64>,
    pub validation: EquationReport,
    /// Original operating ratings, including declared Imax, checked after PF.
    /// A nonempty list does not make the electrical PF unconverged.
    pub limit_violations: Vec<EquationCheck>,
}
impl PreparedAcDcPf {
    pub fn new(instance: AcDcPfInstance) -> Result<Self, String> {
        if !matches!(
            instance.profile(),
            AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1
                | AcDcPfProfile::PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1
                | AcDcPfProfile::PowerModelsAcDc0101AcpVscDcDroopSharedVoltageEqualIncrementV1
        ) {
            return Err("Unsupported coupled PF profile".into());
        }
        let equations = ReferenceEquations::from_pf_instance(&instance)?;
        let p = &equations.model.problem;
        let rows = newton::equality_rows(p)?;
        let equalities: Vec<_> = rows.iter().map(|&i| p.con_names[i].clone()).collect();
        let encoded=serde_json::to_vec(&serde_json::json!({"schema":"tellegen-acdc-pf-v1","network":instance.network(),"profile":instance.profile()})).map_err(|e|e.to_string())?;
        let manifest = AcDcPfManifest {
            schema: "tellegen-acdc-pf-preparation-v1",
            profile: instance.profile(),
            powerio_revision: "70025e87d918be195362ca63a48c9a68d2dfb702",
            reference_revision: "9ada4082ac087aed649f48fb5f977680b0fe06c7",
            model_fingerprint: crate::content_id::content_id(&encoded),
            variables: p.var_names.clone(),
            controls: equalities
                .iter()
                .filter(|n| n.starts_with("pf/") || n.ends_with("setpoint"))
                .cloned()
                .collect(),
            equalities,
            angle_references: equations.angle_references.clone(),
            bus_type_policy: equations.bus_type_policy.clone(),
            limit_policy:
                "post_solve_original_ratings_and_declared_current_no_clipping_or_control_switching",
        };
        let mut limit_network = instance.network().clone();
        for g in limit_network.generators_mut() {
            g.cost = Some(powerio::GenCost::new(2, 0., 0., vec![0.]));
        }
        let adapter = powerio::AcDcOpfInstance::from_network(
            limit_network,
            AcDcOpfProfile::DeclaredCurrentAcpVscV1,
        )
        .map_err(|e| e.to_string())?;
        let limits = ReferenceEquations::from_instance(&adapter, EquationMode::Opf)?;
        let declared = declared_start(&instance, &equations);
        Ok(Self {
            instance: Arc::new(instance),
            adapter,
            equations,
            limits,
            manifest,
            declared,
        })
    }
    pub fn manifest(&self) -> &AcDcPfManifest {
        &self.manifest
    }
    pub fn evaluate(&self, point: &BTreeMap<String, f64>) -> Result<EquationReport, String> {
        self.equations.evaluate(point)
    }
    pub fn initial_point(&self, start: &AcDcPfStart) -> Result<BTreeMap<String, f64>, String> {
        match start {
            AcDcPfStart::Declared => Ok(self.declared.clone()),
            AcDcPfStart::Flat => Ok(self
                .manifest
                .variables
                .iter()
                .map(|n| {
                    let value = if voltage(n) {
                        1.
                    } else if n.ends_with("/iconv") {
                        0.1
                    } else {
                        0.
                    };
                    (n.clone(), value)
                })
                .collect()),
            AcDcPfStart::InstancePoint => coordinates::decode(
                &self.adapter,
                self.instance
                    .initial_point()
                    .ok_or("PF instance has no point")?,
            ),
            AcDcPfStart::Point {
                model_fingerprint,
                values,
            } => {
                if model_fingerprint != &self.manifest.model_fingerprint {
                    return Err("Start belongs to a different PF model/profile".into());
                }
                self.equations.point_values(values)?;
                Ok(values.clone())
            }
        }
    }
    pub fn solve(
        &self,
        start: AcDcPfStart,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<AcDcPfResult, String> {
        let initial_point = self.initial_point(&start)?;
        let initial = self.equations.point_values(&initial_point)?;
        let (x, iterations, current_retractions) =
            newton::solve(&self.equations.model.problem, initial, cancel.as_deref())?;
        let point: BTreeMap<_, _> = self.manifest.variables.iter().cloned().zip(x).collect();
        let validation = self.evaluate(&point)?;
        if validation.max_violation > 1e-9 {
            return Err(format!(
                "PF post-solve validation failed: {}",
                validation.max_violation
            ));
        }
        let limits = self.limits.evaluate(&point)?;
        // Electrical equalities are independently checked above. Ratings do not
        // enter the Newton system, and their violation must remain observable.
        let rows = &self.limits.model.rows;
        let ratings: std::collections::BTreeSet<_> = rows
            .angle_difference
            .iter()
            .chain(&rows.thermal_from)
            .chain(&rows.thermal_to)
            .flatten()
            .map(|&i| self.limits.model.problem.con_names[i].as_str())
            .collect();
        let limit_violations = limits
            .bounds
            .into_iter()
            .chain(
                limits
                    .equations
                    .into_iter()
                    .filter(|e| ratings.contains(e.name.as_str())),
            )
            .filter(|e| e.violation > 1e-7)
            .collect();
        let typed = coordinates::encode(&self.adapter, &point)?;
        let solution = AcDcPfSolution::new(
            Arc::clone(&self.instance),
            typed,
            powerio::Termination::Converged,
            Some("tellegen/acdc-damped-newton-v2".into()),
        )
        .map_err(|e| e.to_string())?;
        Ok(AcDcPfResult {
            solution,
            model_fingerprint: self.manifest.model_fingerprint.clone(),
            solver: "analytic_jacobian_damped_newton_current_retraction_v2",
            iterations,
            current_retractions,
            residual_tolerance: newton::TOLERANCE,
            initial_point,
            point,
            validation,
            limit_violations,
        })
    }
}
pub(super) fn voltage(name: &str) -> bool {
    name.starts_with("vm[")
        || name.ends_with("/vm")
        || name.ends_with("/vmfilt")
        || name.ends_with("/vmconv")
}
fn declared_start(
    instance: &AcDcPfInstance,
    equations: &ReferenceEquations,
) -> BTreeMap<String, f64> {
    let p = &equations.model.problem;
    let mut point: BTreeMap<_, _> = p
        .var_names
        .iter()
        .cloned()
        .zip(p.x0.iter().copied())
        .collect();
    let net = instance.network();
    let base = net.base_mva();
    for g in net.generators().iter().filter(|g| g.in_service) {
        for (field, value) in [("pg", g.pg / base), ("qg", g.qg / base)] {
            let key = format!("{field}[{}]", g.uid.as_deref().unwrap_or(""));
            if let Some(v) = point.get_mut(&key) {
                *v = value;
            }
        }
    }
    for s in net
        .ac_dc_equivalent()
        .into_iter()
        .flat_map(|e| e.stations.iter())
        .filter(|s| s.in_service)
    {
        let pg = s.settings.pcc_withdrawal_mw / base;
        let qg = s.settings.pcc_withdrawal_mvar / base;
        let current = pg.hypot(qg).max(0.1);
        let policy = equations
            .policy
            .iter()
            .find(|p| p.component == s.component.to_string())
            .expect("prepared station");
        let [a, b, c] = policy.loss_coefficients_pu;
        for (field, value) in [
            ("pgrid", pg),
            ("qgrid", qg),
            ("ptf_to", -pg),
            ("qtf_to", -qg),
            ("ppr_fr", pg),
            ("qpr_fr", qg),
            ("pconv", pg),
            ("qconv", qg),
            ("iconv", current),
            ("pdc", a + b * current + c * current * current - pg),
        ] {
            point.insert(format!("station/{}/{field}", s.component), value);
        }
    }
    point
}
pub(super) fn generator_sharing(
    net: &BalancedNetwork,
    prep: &powerio_matrix::AcOpfPreparation,
    cols: &AcOpfColumns,
    p: &mut NlProblemParts,
) -> Result<(), String> {
    for bus in net
        .buses()
        .iter()
        .filter(|b| b.kind != powerio::BusType::Isolated)
    {
        let mut generators: Vec<_> = net
            .generators()
            .iter()
            .filter(|g| g.in_service && g.bus == bus.id)
            .collect();
        generators.sort_by_key(|g| g.uid.as_deref());
        let Some(first) = generators.first() else {
            continue;
        };
        let index = |g: &powerio::Generator| {
            prep.generators
                .identities
                .iter()
                .position(|id| Some(id.as_str()) == g.uid.as_deref())
                .ok_or("PF generator map missing")
        };
        let anchor = index(first)?;
        for g in generators.iter().skip(1) {
            let j = index(g)?;
            for (field, columns, target) in [
                ("q", &cols.qg, (g.qg - first.qg) / net.base_mva()),
                ("p", &cols.pg, (g.pg - first.pg) / net.base_mva()),
            ] {
                if field == "p" && bus.kind != powerio::BusType::Ref {
                    continue;
                }
                setpoint(
                    p,
                    format!("pf/share_{field}[{}]", g.uid.as_deref().unwrap_or("")),
                    subtract(variable(columns[j]), variable(columns[anchor])),
                    target,
                );
            }
        }
    }
    Ok(())
}

/// Complete reactive allocation in PCC injection coordinates. Existing generator
/// sharing handles generator pairs; this adds each voltage-controlling converter.
pub(super) fn voltage_sharing(net: &BalancedNetwork, p: &mut NlProblemParts) -> Result<(), String> {
    let owner = net.ac_dc_equivalent().ok_or("AC/DC owner missing")?;
    for bus in net.buses() {
        let generator = net
            .generators()
            .iter()
            .filter(|g| g.in_service && g.bus == bus.id)
            .min_by_key(|g| g.uid.as_deref());
        let mut stations: Vec<_> = owner
            .stations
            .iter()
            .filter(|s| {
                s.in_service
                    && s.ac_bus == bus.id
                    && s.ac_control == AcConverterControl::VoltageAtPcc
            })
            .collect();
        stations.sort_by_key(|s| s.component.to_string());
        let Some(first) = stations.first() else {
            continue;
        };
        for (j, s) in stations.iter().enumerate() {
            let coordinate = |name: String| {
                p.var_names
                    .iter()
                    .position(|n| *n == name)
                    .map(variable)
                    .ok_or_else(|| format!("Missing PF sharing coordinate {name}"))
            };
            let q = coordinate(format!("station/{}/qgrid", s.component))?;
            let (expression, target) = if let Some(g) = generator {
                // Qg is injection; converter Qgrid is withdrawal.
                (
                    add(
                        coordinate(format!("qg[{}]", g.uid.as_deref().unwrap_or("")))?,
                        q,
                    ),
                    (g.qg + s.settings.pcc_withdrawal_mvar) / net.base_mva(),
                )
            } else {
                if j == 0 {
                    continue;
                }
                (
                    subtract(q, coordinate(format!("station/{}/qgrid", first.component))?),
                    (s.settings.pcc_withdrawal_mvar - first.settings.pcc_withdrawal_mvar)
                        / net.base_mva(),
                )
            };
            setpoint(
                p,
                format!("pf/share_q_voltage[{}]", s.component),
                expression,
                target,
            );
        }
    }
    Ok(())
}
