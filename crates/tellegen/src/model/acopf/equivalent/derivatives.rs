//! Exhaustive small-model derivative diagnostics, including structural zeros.
use super::*;
use pounce_rs::SparsityRequest;

// Fourth-order differences permit larger steps without thermal-row truncation
// while avoiding cancellation of small marginal costs against large objectives.
const STEPS: [f64; 2] = [1e-4, 3e-5];
const TOLERANCE: f64 = 1e-6;

/// One dense comparison. The worst entry is selected by scaled error;
/// max_absolute_error is the maximum over all entries independently.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DerivativeCheck {
    pub name: String,
    pub relative_step: f64,
    pub entries: usize,
    pub max_absolute_error: f64,
    pub max_scaled_error: f64,
    pub worst_row: String,
    pub worst_column: String,
    pub analytic: f64,
    pub finite_difference: f64,
}

#[derive(Debug, serde::Serialize)]
pub struct DerivativeReport {
    pub variables: usize,
    /// All expression rows, including OPF rows disabled in PF. Their Hessian
    /// weights are zero in PF, but their Jacobian entries are still checked.
    pub expression_rows: usize,
    pub jacobian_nonzeros: usize,
    pub hessian_lower_nonzeros: usize,
    pub scaled_tolerance: f64,
    pub finite_difference_order: u8,
    pub passed: bool,
    pub checks: Vec<DerivativeCheck>,
}

impl DerivativeCheck {
    fn new(name: &str, relative_step: f64) -> Self {
        Self {
            name: name.into(),
            relative_step,
            entries: 0,
            max_absolute_error: 0.0,
            max_scaled_error: 0.0,
            worst_row: String::new(),
            worst_column: String::new(),
            analytic: 0.0,
            finite_difference: 0.0,
        }
    }
    fn observe(
        &mut self,
        row: &str,
        column: &str,
        analytic: f64,
        finite: f64,
    ) -> Result<(), String> {
        let absolute = (analytic - finite).abs();
        let scaled = absolute / analytic.abs().max(finite.abs()).max(1.0);
        if !absolute.is_finite() || !scaled.is_finite() {
            return Err(format!(
                "{}: nonfinite derivative at {row}/{column}",
                self.name
            ));
        }
        self.max_absolute_error = self.max_absolute_error.max(absolute);
        if self.entries == 0 || scaled > self.max_scaled_error {
            self.max_scaled_error = scaled;
            self.worst_row = row.into();
            self.worst_column = column.into();
            self.analytic = analytic;
            self.finite_difference = finite;
        }
        self.entries += 1;
        Ok(())
    }
}

struct Structure {
    jac: Vec<(usize, usize)>,
    hess: Vec<(usize, usize)>,
}
fn indices(
    rows: &[i32],
    columns: &[i32],
    m: usize,
    n: usize,
    lower: bool,
) -> Result<Vec<(usize, usize)>, String> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .zip(columns)
        .map(|(&r, &c)| {
            let pair = (
                usize::try_from(r).map_err(|_| "Negative derivative row")?,
                usize::try_from(c).map_err(|_| "Negative derivative column")?,
            );
            if pair.0 >= m || pair.1 >= n || (lower && pair.0 < pair.1) || !seen.insert(pair) {
                return Err("Invalid or duplicate sparse derivative coordinate".into());
            }
            Ok(pair)
        })
        .collect()
}
impl Structure {
    fn read(nlp: &mut NlTnlp, p: &NlProblem) -> Result<Self, String> {
        let info = nlp.get_nlp_info().ok_or("Missing NLP dimensions")?;
        if (usize::try_from(info.n), usize::try_from(info.m)) != (Ok(p.n), Ok(p.m)) {
            return Err("NLP dimensions differ from expression model".into());
        }
        let mut jr = vec![0; usize::try_from(info.nnz_jac_g).map_err(|_| "Invalid Jacobian size")?];
        let mut jc = vec![0; jr.len()];
        let mut hr = vec![0; usize::try_from(info.nnz_h_lag).map_err(|_| "Invalid Hessian size")?];
        let mut hc = vec![0; hr.len()];
        if !nlp.eval_jac_g(
            None,
            false,
            SparsityRequest::Structure {
                irow: &mut jr,
                jcol: &mut jc,
            },
        ) || !nlp.eval_h(
            None,
            false,
            1.0,
            None,
            false,
            SparsityRequest::Structure {
                irow: &mut hr,
                jcol: &mut hc,
            },
        ) {
            return Err("Could not read derivative sparsity".into());
        }
        Ok(Self {
            jac: indices(&jr, &jc, p.m, p.n, false)?,
            hess: indices(&hr, &hc, p.n, p.n, true)?,
        })
    }
    fn first(
        &self,
        nlp: &mut NlTnlp,
        p: &NlProblem,
        x: &[f64],
    ) -> Result<(Vec<f64>, Vec<f64>), String> {
        let mut gradient = vec![0.0; p.n];
        let mut values = vec![0.0; self.jac.len()];
        if !nlp.eval_grad_f(x, true, &mut gradient)
            || !nlp.eval_jac_g(
                Some(x),
                true,
                SparsityRequest::Values {
                    values: &mut values,
                },
            )
        {
            return Err("First derivative evaluation failed".into());
        }
        let mut jacobian = vec![0.0; p.m * p.n];
        for (&(r, c), &v) in self.jac.iter().zip(&values) {
            jacobian[r * p.n + c] = v;
        }
        if gradient.iter().chain(&jacobian).any(|v| !v.is_finite()) {
            return Err("Nonfinite first derivative".into());
        }
        Ok((gradient, jacobian))
    }
}

struct Hessian {
    name: String,
    objective: f64,
    lambda: Vec<f64>,
    values: Vec<f64>,
}
struct Exact {
    gradient: Vec<f64>,
    jacobian: Vec<f64>,
    hessians: Vec<Hessian>,
}

fn family(name: &str) -> &str {
    if name.contains("/transformer/") {
        "transformer"
    } else if name.contains("/reactor/") {
        "reactor"
    } else if name.contains("/filter_") {
        "filter"
    } else if name.ends_with("/current") {
        "valve_current"
    } else if name.ends_with("/loss") {
        "converter_loss"
    } else if name.starts_with("dc_branch/") {
        "dc_branch"
    } else if name.starts_with("dc_bus/") {
        "dc_balance"
    } else if name.starts_with("p_balance[") || name.starts_with("q_balance[") {
        "ac_balance"
    } else if name.starts_with("ac_branch/") {
        "ac_branch"
    } else if name.starts_with("thermal_") {
        "ac_thermal"
    } else {
        "affine_controls"
    }
}

impl Exact {
    fn evaluate(nlp: &mut NlTnlp, p: &NlProblem, x: &[f64], s: &Structure) -> Result<Self, String> {
        let (gradient, jacobian) = s.first(nlp, p, x)?;
        let mut weights: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut all = vec![0.0; p.m];
        for (i, name) in p.con_names.iter().enumerate() {
            if p.g_l[i] <= -INF && p.g_u[i] >= INF {
                continue;
            }
            let weight = (1 + (i * 17 % 11)) as f64 / 11.0 * if i % 2 == 0 { 1.0 } else { -1.0 };
            all[i] = weight;
            weights
                .entry(family(name).into())
                .or_insert_with(|| vec![0.0; p.m])[i] = weight;
        }
        let mut blocks = vec![
            ("objective".into(), 1.0, vec![0.0; p.m]),
            ("lagrangian".into(), 0.7, all),
        ];
        blocks.extend(
            weights
                .into_iter()
                .map(|(name, lambda)| (name, 0.0, lambda)),
        );
        let mut hessians = Vec::new();
        for (name, objective, lambda) in blocks {
            let mut values = vec![0.0; s.hess.len()];
            if !nlp.eval_h(
                Some(x),
                true,
                objective,
                Some(&lambda),
                true,
                SparsityRequest::Values {
                    values: &mut values,
                },
            ) || values.iter().any(|v| !v.is_finite())
            {
                return Err(format!("Hessian evaluation failed: {name}"));
            }
            let mut dense = vec![0.0; p.n * p.n];
            for (&(r, c), &v) in s.hess.iter().zip(&values) {
                dense[r * p.n + c] = v;
                dense[c * p.n + r] = v;
            }
            hessians.push(Hessian {
                name,
                objective,
                lambda,
                values: dense,
            });
        }
        Ok(Self {
            gradient,
            jacobian,
            hessians,
        })
    }
}

// Keep comparison separate from extraction so fault regressions can corrupt
// actual case5 derivative arrays without changing the function values/oracle.
fn compare(
    nlp: &mut NlTnlp,
    p: &NlProblem,
    x: &[f64],
    s: &Structure,
    exact: &Exact,
) -> Result<DerivativeReport, String> {
    let mut checks = Vec::new();
    for step in STEPS {
        let mut grad = DerivativeCheck::new("objective_gradient", step);
        let mut jac = DerivativeCheck::new("constraint_jacobian", step);
        let mut hess = exact
            .hessians
            .iter()
            .map(|h| DerivativeCheck::new(&format!("hessian/{}", h.name), step))
            .collect::<Vec<_>>();
        for column in 0..p.n {
            let h = step * x[column].abs().max(1.0);
            let mut sample = |offset: f64| -> Result<_, String> {
                let mut shifted = x.to_vec();
                shifted[column] += offset * h;
                let f = nlp
                    .eval_f(&shifted, true)
                    .ok_or("Objective finite difference failed")?;
                let mut g = vec![0.0; p.m];
                if !nlp.eval_g(&shifted, true, &mut g) {
                    return Err("Constraint finite difference failed".into());
                }
                let (df, j) = s.first(nlp, p, &shifted)?;
                Ok((f, g, df, j))
            };
            let (fp, gp, dfp, jp) = sample(1.0)?;
            let (fm, gm, dfm, jm) = sample(-1.0)?;
            let (fpp, gpp, dfpp, jpp) = sample(2.0)?;
            let (fmm, gmm, dfmm, jmm) = sample(-2.0)?;
            // Pair differences first: avoid multiplying the common large offset.
            let numerator = |a: f64, b: f64, c: f64, d: f64| 8.0 * (a - b) - (c - d);
            grad.observe(
                "objective",
                &p.var_names[column],
                exact.gradient[column],
                numerator(fp, fm, fpp, fmm) / (12.0 * h),
            )?;
            for row in 0..p.m {
                jac.observe(
                    &p.con_names[row],
                    &p.var_names[column],
                    exact.jacobian[row * p.n + column],
                    numerator(gp[row], gm[row], gpp[row], gmm[row]) / (12.0 * h),
                )?;
            }
            for (block, check) in exact.hessians.iter().zip(&mut hess) {
                // Accumulate the difference of Lagrangian gradients once per
                // block through the sparse Jacobian, then compare every dense
                // Hessian entry (including structural zeros). This is the same
                // weighted sum without n*m zero terms for each column/block.
                let mut finite: Vec<_> = (0..p.n)
                    .map(|i| block.objective * numerator(dfp[i], dfm[i], dfpp[i], dfmm[i]))
                    .collect();
                for &(constraint, variable) in &s.jac {
                    let i = constraint * p.n + variable;
                    finite[variable] +=
                        block.lambda[constraint] * numerator(jp[i], jm[i], jpp[i], jmm[i]);
                }
                for (row, &difference) in finite.iter().enumerate() {
                    check.observe(
                        &p.var_names[row],
                        &p.var_names[column],
                        block.values[row * p.n + column],
                        difference / (12.0 * h),
                    )?;
                }
            }
        }
        checks.push(grad);
        checks.push(jac);
        checks.extend(hess);
    }
    let passed = checks.iter().all(|c| c.max_scaled_error <= TOLERANCE);
    Ok(DerivativeReport {
        variables: p.n,
        expression_rows: p.m,
        jacobian_nonzeros: s.jac.len(),
        hessian_lower_nonzeros: s.hess.len(),
        scaled_tolerance: TOLERANCE,
        finite_difference_order: 4,
        passed,
        checks,
    })
}

impl ReferenceEquations {
    /// Exhaustive finite differences for small diagnostic models. Checks every
    /// Jacobian/Hessian entry, including zeros outside reported sparsity. Hessians
    /// are checked against differences of independently checked first derivatives,
    /// for the objective, a mixed Lagrangian and each active equation family.
    /// Perturbations are deliberately not clamped to bounds or feasibility.
    pub fn check_derivatives(
        &self,
        point: &BTreeMap<String, f64>,
    ) -> Result<DerivativeReport, String> {
        let x = self.point_values(point)?;
        let p = &self.model.problem;
        let mut nlp = self.model.tnlp()?;
        let s = Structure::read(&mut nlp, p)?;
        let exact = Exact::evaluate(&mut nlp, p, &x, &s)?;
        compare(&mut nlp, p, &x, &s, &exact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    mod reference {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/acdc_reference.rs"
        ));
    }

    #[test]
    fn derivative_checker_rejects_corrupted_case5_sparse_values() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc");
        let (net, oracle, map) = reference::load(&dir);
        let model = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf).unwrap();
        let point = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
        let x = model.point_values(&point).unwrap();
        let p = &model.model.problem;
        let mut nlp = model.model.tnlp().unwrap();
        let structure = Structure::read(&mut nlp, p).unwrap();
        for fault in [
            "gradient",
            "missing_jacobian",
            "spurious_jacobian_zero",
            "hessian_sign",
            "spurious_hessian_zero",
        ] {
            let mut exact = Exact::evaluate(&mut nlp, p, &x, &structure).unwrap();
            let expected = match fault {
                "gradient" => {
                    *exact.gradient.iter_mut().find(|v| v.abs() > 1.0).unwrap() = 0.0;
                    "objective_gradient"
                }
                "missing_jacobian" => {
                    *exact.jacobian.iter_mut().find(|v| v.abs() > 1.0).unwrap() = 0.0;
                    "constraint_jacobian"
                }
                "spurious_jacobian_zero" => {
                    let absent = (0..p.m * p.n)
                        .find(|i| !structure.jac.contains(&(i / p.n, i % p.n)))
                        .unwrap();
                    exact.jacobian[absent] = 1.0;
                    "constraint_jacobian"
                }
                "hessian_sign" => {
                    let block = exact
                        .hessians
                        .iter_mut()
                        .find(|h| h.name == "valve_current")
                        .unwrap();
                    let offset = (0..p.n * p.n)
                        .find(|&i| i / p.n != i % p.n && block.values[i].abs() > 0.01)
                        .unwrap();
                    let transpose = (offset % p.n) * p.n + offset / p.n;
                    block.values[offset] *= -1.0;
                    block.values[transpose] *= -1.0;
                    "hessian/valve_current"
                }
                "spurious_hessian_zero" => {
                    let block = exact
                        .hessians
                        .iter_mut()
                        .find(|h| h.name == "objective")
                        .unwrap();
                    let absent = (0..p.n)
                        .find(|i| !structure.hess.contains(&(*i, *i)))
                        .unwrap();
                    block.values[absent * p.n + absent] = 1.0;
                    "hessian/objective"
                }
                _ => unreachable!(),
            };
            let report = compare(&mut nlp, p, &x, &structure, &exact).unwrap();
            assert!(!report.passed, "{fault} was not detected");
            assert!(
                report
                    .checks
                    .iter()
                    .filter(|c| c.name == expected)
                    .all(|c| c.max_scaled_error > TOLERANCE),
                "{fault} must fail at both step sizes"
            );
        }
    }

    #[test]
    fn quadratic_cost_has_nonzero_validated_curvature() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc");
        let (mut net, oracle, map) = reference::load(&dir);
        // A controlled native-only mutation: the original case has linear costs
        // and cannot qualify a nonzero objective Hessian.
        net.generators_mut()[0].cost.as_mut().unwrap().coeffs[0] = 0.02;
        let model = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf).unwrap();
        let point = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
        let x = model.point_values(&point).unwrap();
        let p = &model.model.problem;
        let mut nlp = model.model.tnlp().unwrap();
        let structure = Structure::read(&mut nlp, p).unwrap();
        let exact = Exact::evaluate(&mut nlp, p, &x, &structure).unwrap();
        let curvature = exact
            .hessians
            .iter()
            .find(|h| h.name == "objective")
            .unwrap()
            .values
            .iter()
            .copied()
            .fold(0.0, f64::max);
        assert!((curvature - 400.0).abs() < 1e-10);
        assert!(compare(&mut nlp, p, &x, &structure, &exact).unwrap().passed);
    }

    #[test]
    fn sparse_structure_and_nonfinite_comparisons_fail_closed() {
        assert!(indices(&[-1], &[0], 1, 1, false).is_err());
        assert!(indices(&[0, 0], &[0, 0], 1, 1, false).is_err());
        assert!(indices(&[1], &[0], 1, 1, false).is_err());
        assert!(indices(&[0], &[1], 2, 2, true).is_err());
        let mut check = DerivativeCheck::new("nonfinite", 1e-5);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(check.observe("row", "column", bad, 0.0).is_err());
            assert!(check.observe("row", "column", 0.0, bad).is_err());
        }
    }
}
