//! Evaluate pinned independent AC/DC solutions in native Tellegen equations.
#[path = "../tests/support/acdc_reference.rs"]
mod reference;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "evals/validation/acdc".into());
    let (net, oracle, map) = reference::load(std::path::Path::new(&dir));
    let mut reports = serde_json::Map::new();
    for (label, mode) in [("pf", EquationMode::Pf), ("opf", EquationMode::Opf)] {
        if !reference::qualified(&oracle, label) {
            eprintln!("Skipping {label}: reference solve is explicitly unqualified");
            continue;
        }
        let model =
            ReferenceEquations::from_instance(&reference::instance(net.clone(), &map), mode)?;
        let point = reference::point(&net, &oracle["solves"][label]["solution"], &map);
        let (point, shifts) = model.align_angle_references(&point)?;
        eprintln!(
            "{label} angle alignment: {}",
            serde_json::to_string(&shifts)?
        );
        let report = model.evaluate(&point)?;
        if report.max_violation > 1e-7 {
            return Err(format!(
                "{label}: native violation {}; rows {:?}",
                report.max_violation,
                report
                    .equations
                    .iter()
                    .chain(&report.bounds)
                    .filter(|r| r.violation > 1e-7)
                    .collect::<Vec<_>>()
            )
            .into());
        }
        if mode == EquationMode::Opf
            && (report.objective
                - oracle["solves"][label]["objective"]
                    .as_f64()
                    .ok_or("missing objective")?)
            .abs()
                > 1e-7
        {
            return Err("Objective mismatch".into());
        }
        let mut report = serde_json::to_value(report)?;
        report["reference_angle_alignment"] = serde_json::to_value(shifts)?;
        reports.insert(label.into(), report);
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(())
}
