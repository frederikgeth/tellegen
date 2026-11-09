//! Native case5 OPF from reference, declared-data and independent flat starts.
#[path = "../tests/support/acdc_reference.rs"]
mod reference;
use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfStart, PreparedAcDcOpf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "evals/validation/acdc".into());
    let (net, oracle, map) = reference::load(std::path::Path::new(&dir));
    let instance = powerio::AcDcOpfInstance::from_network(
        net.clone(),
        AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
    )?;
    let prepared = PreparedAcDcOpf::new(instance)?;
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
        let module = powerio::PioModule::new(powerio::PioValue::from(solution.solution.clone()));
        let output = powerio::serialize(&module, powerio::Destination::memory("result.json")?)?;
        let powerio::EmittedOutput::Memory { mut artifacts } = output.into_output() else {
            return Err("Expected IR bytes".into());
        };
        std::fs::write(
            std::path::Path::new(&dir).join(format!("native-{name}.pio.json")),
            artifacts.pop().ok_or("Missing IR artifact")?.into_bytes(),
        )?;
        solutions.insert(name.into(), serde_json::to_value(solution)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"profile":prepared.instance().profile(),"preparation":prepared.manifest(),"solves":solutions})
        )?
    );
    Ok(())
}
