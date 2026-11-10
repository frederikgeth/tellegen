//! Small-system reference-quality Newton solve; no hidden slack or regularization.
use super::*;
use pounce_rs::SparsityRequest;
pub(super) const TOLERANCE: f64 = 1e-10;
pub(super) fn equality_rows(p: &NlProblem) -> Result<Vec<usize>, String> {
    let rows: Vec<_> = (0..p.m)
        .filter(|&i| p.g_l[i] > -INF || p.g_u[i] < INF)
        .collect();
    if rows.iter().any(|&i| p.g_l[i] != p.g_u[i]) || rows.len() != p.n {
        return Err(format!(
            "PF control system is not square: {} active rows, {} variables",
            rows.len(),
            p.n
        ));
    }
    Ok(rows)
}
fn residual(
    nlp: &mut NlTnlp,
    p: &NlProblem,
    rows: &[usize],
    x: &[f64],
) -> Result<Vec<f64>, String> {
    let mut g = vec![0.; p.m];
    if !nlp.eval_g(x, true, &mut g) || g.iter().any(|v| !v.is_finite()) {
        return Err("Nonfinite PF equations".into());
    }
    Ok(rows.iter().map(|&i| g[i] - p.g_l[i]).collect())
}
fn norm(x: &[f64]) -> f64 {
    x.iter().map(|v| v.abs()).fold(0., f64::max)
}
fn linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Result<Vec<f64>, String> {
    let n = b.len();
    for (row, rhs) in a.iter_mut().zip(&mut b) {
        let scale = norm(row);
        if !scale.is_finite() || scale == 0. {
            return Err("Singular PF Jacobian: zero/nonfinite row".into());
        }
        for v in row {
            *v /= scale;
        }
        *rhs /= scale;
    }
    for k in 0..n {
        let pivot = (k..n)
            .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
            .ok_or("Empty PF pivot")?;
        if !a[pivot][k].is_finite() || a[pivot][k].abs() < 1e-12 {
            return Err(format!("Singular/ill-conditioned PF Jacobian at column {k}; no solution or infeasibility claim"));
        }
        a.swap(k, pivot);
        b.swap(k, pivot);
        let (pivot_rows, remaining) = a.split_at_mut(k + 1);
        let pivot_row = &pivot_rows[k];
        for (offset, row) in remaining.iter_mut().enumerate() {
            let factor = row[k] / pivot_row[k];
            row[k] = 0.;
            for (value, pivot_value) in row.iter_mut().zip(pivot_row).skip(k + 1) {
                *value -= factor * pivot_value;
            }
            b[k + 1 + offset] -= factor * b[k];
        }
    }
    let mut x = vec![0.; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|j| a[i][j] * x[j]).sum::<f64>()) / a[i][i];
    }
    if x.iter().any(|v| !v.is_finite()) {
        return Err("Nonfinite PF Newton direction".into());
    }
    Ok(x)
}
pub(super) fn solve(
    p: &NlProblem,
    mut x: Vec<f64>,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<f64>, usize, usize), String> {
    let rows = equality_rows(p)?;
    let mut nlp = NlTnlp::try_new(p.clone())?;
    let info = nlp
        .get_nlp_info()
        .ok_or("PF derivative dimensions missing")?;
    let nnz = usize::try_from(info.nnz_jac_g).map_err(|_| "Invalid PF derivative size")?;
    let mut jr = vec![0; nnz];
    let mut jc = vec![0; nnz];
    if !nlp.eval_jac_g(
        None,
        false,
        SparsityRequest::Structure {
            irow: &mut jr,
            jcol: &mut jc,
        },
    ) {
        return Err("PF derivative structure missing".into());
    }
    let mut selected = vec![None; p.m];
    for (i, &r) in rows.iter().enumerate() {
        selected[r] = Some(i);
    }
    let domain = |x: &[f64]| {
        x.iter().zip(&p.var_names).all(|(&v, n)| {
            v.is_finite() && (!voltage(n) || v > 1e-8) && (!n.ends_with("/iconv") || v >= 0.)
        })
    };
    if !domain(&x) {
        return Err("PF start needs positive voltage magnitudes and nonnegative current".into());
    }
    // The squared current equality has an unwanted negative-current root.
    // A Newton step towards it can pin the line search at I=0 near idle.
    // Retraction to I=hypot(Pvalve,Qvalve)/Vvalve preserves that equality and
    // the physical root; every candidate still passes the full residual test.
    let currents: Vec<_> = p
        .var_names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            n.strip_suffix("/iconv").map(|prefix| {
                let col = |field| {
                    p.var_names
                        .iter()
                        .position(|v| v == &format!("{prefix}/{field}"))
                        .ok_or_else(|| format!("Missing PF valve coordinate {prefix}/{field}"))
                };
                Ok((i, col("pconv")?, col("qconv")?, col("vmconv")?))
            })
        })
        .collect::<Result<_, String>>()?;
    let mut current_retractions = 0;
    for iteration in 0..=60 {
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err("AC/DC PF cancelled".into());
        }
        let g = residual(&mut nlp, p, &rows, &x)?;
        let error = norm(&g);
        let mut values = vec![0.; nnz];
        if !nlp.eval_jac_g(
            Some(&x),
            true,
            SparsityRequest::Values {
                values: &mut values,
            },
        ) {
            return Err("PF derivative evaluation failed".into());
        }
        let mut jac = vec![vec![0.; p.n]; p.n];
        for ((&r, &c), v) in jr.iter().zip(&jc).zip(values) {
            let r = usize::try_from(r).map_err(|_| "Negative Jacobian row")?;
            let c = usize::try_from(c).map_err(|_| "Negative Jacobian column")?;
            if r >= p.m || c >= p.n || !v.is_finite() {
                return Err("Invalid PF Jacobian entry".into());
            }
            if let Some(i) = selected[r] {
                jac[i][c] += v;
            }
        }
        // Even a supplied converged point must have a nonsingular control Jacobian.
        let direction = linear(jac, g.iter().map(|v| -v).collect())?;
        if error < TOLERANCE {
            return Ok((x, iteration, current_retractions));
        }
        if iteration == 60 {
            break;
        }
        let mut alpha = 1.;
        let mut accepted = None;
        for _ in 0..30 {
            let mut trial: Vec<_> = x
                .iter()
                .zip(&direction)
                .map(|(a, d)| a + alpha * d)
                .collect();
            if domain(&trial) {
                if let Ok(g) = residual(&mut nlp, p, &rows, &trial) {
                    if norm(&g) < (1. - 1e-4 * alpha) * error {
                        accepted = Some(trial);
                        break;
                    }
                }
            }
            // Prefer the ordinary Newton candidate; use the exact current
            // relation only when that candidate was inadmissible or rejected.
            for &(i, pc, qc, vm) in &currents {
                trial[i] = trial[pc].hypot(trial[qc]) / trial[vm];
            }
            if domain(&trial) {
                if let Ok(g) = residual(&mut nlp, p, &rows, &trial) {
                    if norm(&g) < (1. - 1e-4 * alpha) * error {
                        current_retractions += 1;
                        accepted = Some(trial);
                        break;
                    }
                }
            }
            alpha *= 0.5;
        }
        x = accepted.ok_or_else(|| {
            format!("PF line search stalled at residual {error}; no infeasibility claim")
        })?;
    }
    Err("PF iteration limit; no infeasibility claim".into())
}
