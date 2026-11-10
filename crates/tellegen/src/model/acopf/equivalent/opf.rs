//! Native solver adapter over canonical PowerIO coupled intent and preparation.
use super::*;
use serde::{Deserialize, Serialize};

pub use powerio::AcDcOpfProfile;

/// Starts are numerical choices, not PF dispatch constraints or observations.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcDcOpfStart {
    Declared,
    /// Use the canonical instance's supplied typed operating point.
    InstancePoint,
    Flat,
    Point {
        model_fingerprint: String,
        values: BTreeMap<String, f64>,
    },
}

/// A bounded scalar descriptor, never a dense solver row/column index.
#[derive(Debug, Serialize)]
pub struct AcDcScalar {
    pub name: String,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct AcDcOpfManifest {
    pub schema: &'static str,
    pub profile: AcDcOpfProfile,
    pub current_limit_policy: AcDcCurrentLimitPolicy,
    pub coordinate_policy: AcDcCoordinatePolicy,
    pub dc_branch_policy: Vec<AcDcBranchPolicy>,
    pub model_fingerprint: String,
    pub powerio_revision: &'static str,
    pub reference_revision: &'static str,
    pub base_mva: f64,
    pub pole_count: u8,
    pub objective: &'static str,
    pub dispatch: &'static str,
    pub units: &'static str,
    pub ac_bus_ids: Vec<usize>,
    pub generator_ids: Vec<String>,
    pub ac_branch_ids: Vec<String>,
    pub dc_bus_ids: Vec<String>,
    pub dc_branch_ids: Vec<String>,
    pub station_ids: Vec<String>,
    pub variables: Vec<AcDcScalar>,
    pub constraints: Vec<AcDcScalar>,
    pub station_policy: Vec<StationPolicy>,
    pub angle_reference_policy: AcDcAngleReferencePolicy,
    pub angle_references: Vec<AcDcAngleReference>,
}

/// Immutable preparation keeps source declarations separate from effective
/// policy/bounds. The native expression DAG remains private to Tellegen.
pub struct PreparedAcDcOpf {
    equations: ReferenceEquations,
    instance: Arc<powerio::AcDcOpfInstance>,
    manifest: AcDcOpfManifest,
    declared: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
pub struct AcDcOpfResult {
    /// Canonical physical result, serialized through PowerIO IR by the caller.
    #[serde(skip)]
    pub solution: powerio::AcDcOpfSolution,
    pub schema: &'static str,
    pub model_fingerprint: String,
    pub termination: &'static str,
    pub solver: &'static str,
    pub solver_options: String,
    pub base_restoration_calls: usize,
    pub solver_status: String,
    pub base_status: String,
    pub recovery_attempts: Vec<String>,
    pub promoted_by: Option<String>,
    pub iterations: usize,
    pub restoration_calls: usize,
    pub solver_constraint_violation: f64,
    pub solver_kkt_error: f64,
    pub start: AcDcOpfStart,
    pub initial_point: BTreeMap<String, f64>,
    pub point: BTreeMap<String, f64>,
    pub validation: EquationReport,
}

impl PreparedAcDcOpf {
    pub fn new(instance: powerio::AcDcOpfInstance) -> Result<Self, String> {
        let net = instance.network();
        // The caller selects InstancePoint explicitly; declared and flat starts
        // must remain independent of any point attached to the canonical intent.
        let source_intent = powerio::AcDcOpfInstance::from_network(net.clone(), instance.profile())
            .map_err(|e| e.to_string())?
            .with_angle_reference_policy(instance.angle_reference_policy());
        let equations = ReferenceEquations::from_instance(&source_intent, EquationMode::Opf)?;
        let p = &equations.model.problem;
        let prep = &equations.model.preparation;
        let owner = net.ac_dc_equivalent().ok_or("Equivalent owner missing")?;
        // Hash the complete source (including all DC/station declarations), the
        // selected policy and the formulation version, not only the AC projection.
        let encoded = serde_json::to_vec(
            &serde_json::json!({"schema":"tellegen-acdc-opf-v3","network":net,"profile":instance.profile(),"angle_reference_policy":instance.angle_reference_policy(),"angle_references":equations.angle_references}),
        )
        .map_err(|e| e.to_string())?;
        let model_fingerprint = crate::content_id::content_id(&encoded);
        let descriptors = |names: &[String], lo: &[f64], hi: &[f64]| {
            names
                .iter()
                .zip(lo)
                .zip(hi)
                .map(|((name, &lo), &hi)| AcDcScalar {
                    name: name.clone(),
                    lower: (lo > -INF).then_some(lo),
                    upper: (hi < INF).then_some(hi),
                })
                .collect()
        };
        let manifest = AcDcOpfManifest {
            schema: "tellegen-acdc-opf-preparation-v3",
            profile: instance.profile(),
            current_limit_policy: instance.profile().current_limit_policy(),
            coordinate_policy: instance.profile().coordinate_policy(),
            dc_branch_policy: equations.dc_branch_policy.clone(),
            model_fingerprint,
            powerio_revision: "ba1f9fc90de768e53aaace1c21e68b0d64759713",
            reference_revision: "9ada4082ac087aed649f48fb5f977680b0fe06c7",
            base_mva: owner.base_mva,
            pole_count: owner.pole_count,
            objective: "network_generator_cost",
            dispatch: "free_generator_and_vsc_dispatch_no_pf_setpoints",
            units: if instance.profile().coordinate_policy()
                == AcDcCoordinatePolicy::PhysicalLocalBases
            {
                "power_pu_on_base_mva; voltage_pu_local_physical_bases; angle_rad; current_pu_converter_ac_base"
            } else {
                concat!(
                    "power_pu_on_base_mva; voltage_pu_source_coordinates; ",
                    "angle_rad; current_pu_converter_ac_base"
                )
            },
            ac_bus_ids: prep.bus_ids.iter().map(|id| id.0).collect(),
            generator_ids: prep.generators.identities.clone(),
            ac_branch_ids: prep.branches.identities.clone(),
            dc_bus_ids: owner
                .buses
                .iter()
                .map(|b| b.component.to_string())
                .collect(),
            dc_branch_ids: owner
                .branches
                .iter()
                .filter(|b| b.in_service)
                .map(|b| b.component.to_string())
                .collect(),
            station_ids: owner
                .stations
                .iter()
                .filter(|s| s.in_service)
                .map(|s| s.component.to_string())
                .collect(),
            variables: descriptors(&p.var_names, &p.x_l, &p.x_u),
            constraints: descriptors(&p.con_names, &p.g_l, &p.g_u),
            station_policy: equations.policy.clone(),
            angle_reference_policy: instance.angle_reference_policy(),
            angle_references: equations.angle_references.clone(),
        };
        let mut declared: BTreeMap<_, _> = p
            .var_names
            .iter()
            .cloned()
            .zip(p.x0.iter().copied())
            .collect();
        for s in owner.stations.iter().filter(|s| s.in_service) {
            let prefix = format!("station/{}", s.component);
            let pg = s.settings.pcc_withdrawal_mw / owner.base_mva;
            let qg = s.settings.pcc_withdrawal_mvar / owner.base_mva;
            let current = pg.hypot(qg).max(0.1);
            let policy = manifest
                .station_policy
                .iter()
                .find(|v| v.component == s.component.to_string())
                .ok_or("Station policy missing")?;
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
                declared.insert(format!("{prefix}/{field}"), value);
            }
        }
        // Native declaration-based starts may violate equations. Project only
        // numeric starts to declared bounds; explicit supplied points are exact.
        for (i, name) in p.var_names.iter().enumerate() {
            let v = declared.get_mut(name).ok_or("Start variable missing")?;
            *v = v.clamp(p.x_l[i], p.x_u[i]);
        }
        Ok(Self {
            equations,
            instance: Arc::new(instance),
            manifest,
            declared,
        })
    }
    pub fn instance(&self) -> &powerio::AcDcOpfInstance {
        &self.instance
    }
    pub fn operating_point(
        &self,
        point: &BTreeMap<String, f64>,
    ) -> Result<powerio::AcDcOperatingPoint, String> {
        self.equations.point_values(point)?;
        coordinates::encode(&self.instance, point)
    }
    pub fn manifest(&self) -> &AcDcOpfManifest {
        &self.manifest
    }
    pub fn evaluate(&self, point: &BTreeMap<String, f64>) -> Result<EquationReport, String> {
        self.equations.evaluate(point)
    }
    pub fn align_angle_references(
        &self,
        point: &BTreeMap<String, f64>,
    ) -> Result<(BTreeMap<String, f64>, Vec<AcDcAngleShift>), String> {
        self.equations.align_angle_references(point)
    }
    pub fn initial_point(&self, start: &AcDcOpfStart) -> Result<BTreeMap<String, f64>, String> {
        let p = &self.equations.model.problem;
        match start {
            AcDcOpfStart::Point {
                model_fingerprint,
                values,
            } => {
                if *model_fingerprint != self.manifest.model_fingerprint {
                    return Err("Start belongs to a different model/profile fingerprint".into());
                }
                self.equations.point_values(values)?;
                Ok(values.clone())
            }
            AcDcOpfStart::InstancePoint => coordinates::decode(
                &self.instance,
                self.instance
                    .initial_point()
                    .ok_or("Instance has no supplied operating point")?,
            ),
            AcDcOpfStart::Declared => Ok(self.declared.clone()),
            AcDcOpfStart::Flat => Ok(p
                .var_names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let value = if name.starts_with("vm[")
                        || name.ends_with("/vm")
                        || name.ends_with("/vmfilt")
                        || name.ends_with("/vmconv")
                    {
                        1.0
                    } else if name.ends_with("/iconv") {
                        0.1
                    } else if name.starts_with("pg[") {
                        (p.x_l[i] + p.x_u[i]) / 2.0
                    } else {
                        0.0
                    };
                    (name.clone(), value.clamp(p.x_l[i], p.x_u[i]))
                })
                .collect()),
        }
    }
    /// Experimental local OPF only. Retains recovery provenance, requires a
    /// successful solver status and re-evaluates every native row and bound.
    /// Independent formulation checks live in the Julia harness. Global
    /// optimality and unqualified profiles are not inferred.
    pub fn solve(
        &self,
        start: AcDcOpfStart,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<AcDcOpfResult, String> {
        if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err("AC/DC OPF cancelled".into());
        }
        let initial_point = self.initial_point(&start)?;
        let mut problem = self.equations.model.problem.clone();
        problem.x0 = self.equations.point_values(&initial_point)?;
        let target = Rc::new(RefCell::new(CancellableNlTnlp::new(
            NlTnlp::try_new(problem)?,
            cancel.clone(),
        )));
        let mut app = IpoptApplication::new();
        let solver_options = format!("{POUNCE_OPTIONS}bound_relax_factor 0\n");
        app.initialize_with_options_str(&solver_options)
            .map_err(|e| e.to_string())?;
        app.initialize().map_err(|e| e.to_string())?;
        configure_restoration(&mut app);
        let (outcome, base_restoration_calls) = optimize_with_recovery(
            &mut app,
            Rc::clone(&target) as Rc<RefCell<dyn TNLP>>,
            &cancel,
            &mut |_| {},
        )?;
        let stats = &outcome.statistics;
        if !matches!(
            outcome.status,
            ApplicationReturnStatus::SolveSucceeded
                | ApplicationReturnStatus::SolvedToAcceptableLevel
        ) {
            return Err(format!("Local AC/DC OPF stopped: {:?}, iterations {}, constraint violation {}, KKT {}; this is not an infeasibility proof",outcome.status,outcome.total_iteration_count(),stats.final_unscaled_constr_viol,stats.final_unscaled_kkt_error));
        }
        let solved = target.borrow();
        let x = solved
            .inner
            .final_x()
            .ok_or("Solver returned no primal point")?;
        if x.len() != self.equations.variable_names().len() {
            return Err("Solver point has wrong length".into());
        }
        let point: BTreeMap<_, _> = self
            .equations
            .variable_names()
            .iter()
            .cloned()
            .zip(x.iter().copied())
            .collect();
        let validation = self.evaluate(&point)?;
        if validation.max_violation > 1e-7
            || !stats.final_unscaled_constr_viol.is_finite()
            || stats.final_unscaled_constr_viol > 1e-7
            || !stats.final_unscaled_kkt_error.is_finite()
            || stats.final_unscaled_kkt_error > 1e-6
            || !solved.inner.final_obj().is_finite()
            || (validation.objective - solved.inner.final_obj()).abs() > 1e-6
        {
            return Err(format!(
                "AC/DC OPF validation failed: native violation {}, KKT {}, objective difference {}",
                validation.max_violation,
                stats.final_unscaled_kkt_error,
                validation.objective - solved.inner.final_obj()
            ));
        }
        let solution = powerio::AcDcOpfSolution::new(
            Arc::clone(&self.instance),
            self.operating_point(&point)?,
            validation.objective,
            powerio::Termination::Converged,
            Some("tellegen/pounce-0.15.0-feral".into()),
        )
        .map_err(|e| e.to_string())?;
        Ok(AcDcOpfResult {
            solution,
            schema: "tellegen-acdc-opf-result-v3",
            solver: "pounce-0.15.0-feral",
            solver_options,
            base_restoration_calls,
            model_fingerprint: self.manifest.model_fingerprint.clone(),
            termination: "locally_solved",
            solver_status: outcome.status.upstream_name().into(),
            base_status: outcome.base_status.upstream_name().into(),
            recovery_attempts: outcome.tried.iter().map(|s| (*s).into()).collect(),
            promoted_by: outcome.promoted_by.map(str::to_owned),
            iterations: outcome.total_iteration_count(),
            restoration_calls: usize::try_from(stats.restoration_calls).unwrap_or(0),
            solver_constraint_violation: stats.final_unscaled_constr_viol,
            solver_kkt_error: stats.final_unscaled_kkt_error,
            start,
            initial_point,
            point,
            validation,
        })
    }
}
