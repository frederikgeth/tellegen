//! Native case5 OPF from reference, declared-data and independent flat starts.
#[path = "../tests/support/acdc_reference.rs"]
mod reference;
use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfSpec, AcDcOpfStart, PreparedAcDcOpf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "evals/validation/acdc".into());
    let (net, oracle, map) = reference::load(std::path::Path::new(&dir));
    let spec = AcDcOpfSpec {
        profile: AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
    };
    let prepared = PreparedAcDcOpf::new(&net, &spec)?;
    let point = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
    let mut solutions = serde_json::Map::new();
    for (name, start) in [
        (
            "reference",
            AcDcOpfStart::Point {
                model_fingerprint: prepared.manifest().model_fingerprint.clone(),
                values: point,
            },
        ),
        ("declared", AcDcOpfStart::Declared),
        ("flat", AcDcOpfStart::Flat),
    ] {
        eprintln!("Solving case5 from {name} start");
        let solution = prepared.solve(start, None)?;
        eprintln!(
            "{name}: {} iterations; objective {}; violation {}; KKT {}",
            solution.iterations,
            solution.validation.objective,
            solution.validation.max_violation,
            solution.solver_kkt_error
        );
        solutions.insert(name.into(), serde_json::to_value(solution)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"spec":spec,"preparation":prepared.manifest(),"solves":solutions})
        )?
    );
    Ok(())
}
