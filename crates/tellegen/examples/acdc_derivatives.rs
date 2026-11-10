//! Reproduce dense native AC/DC derivative checks without Julia at runtime.
#[path = "../tests/support/acdc_reference.rs"]
mod reference;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "evals/validation/acdc".into());
    let (net, oracle, map) = reference::load(std::path::Path::new(&dir));
    let mut reports = serde_json::Map::new();
    let mut passed = true;
    for (label, mode) in [("pf", EquationMode::Pf), ("opf", EquationMode::Opf)] {
        let model = ReferenceEquations::powermodels_acdc_0101(&net, mode)?;
        let point = reference::point(&net, &oracle["solves"][label]["solution"], &map);
        for (name, point) in reference::derivative_points(&point) {
            eprintln!(
                "Checking {label}/{name}: {} variables",
                model.variable_names().len()
            );
            let report = model.check_derivatives(&point)?;
            eprintln!("{label}/{name}: passed={}", report.passed);
            passed &= report.passed;
            reports.insert(format!("{label}/{name}"), serde_json::to_value(report)?);
        }
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    if !passed {
        return Err("Native derivative checks failed; see report".into());
    }
    Ok(())
}
