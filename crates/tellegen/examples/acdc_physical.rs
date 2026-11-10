//! Export independently constructed physical circuits for the pinned Julia checker.
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use physical::*;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output directory")?);
    std::fs::create_dir_all(&out)?;
    let mut cases = Vec::new();
    let mut inputs = Vec::new();
    for direction in [-1.0, 1.0] {
        for poles in [1, 2] {
            for b in bases() {
                inputs.push((Circuit::new(direction, poles), b, 5.0));
            }
        }
    }
    for mva in [50.0, 100.0, 250.0] {
        for converter_kv in [200.0, 400.0] {
            inputs.push((
                Circuit::new(1.0, 2),
                Bases {
                    mva,
                    pcc_kv: 230.0,
                    converter_kv,
                    dc_kv: 500.0,
                },
                0.05,
            ));
        }
    }
    for (index, (c, b, limit)) in inputs.into_iter().enumerate() {
        let net = c.network(b, limit);
        let point = c.point(b);
        let report =
            ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf)?.evaluate(&point)?;
        assert!(report.equations.iter().all(|e| e.violation < 1e-9));
        let file = format!("physical_{index:02}.m");
        let emitted = powerio::emit(
            &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(net)),
            "matpower-acdc",
            powerio::Destination::memory(&file)?,
        )?;
        let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
            return Err("expected memory output".into());
        };
        assert_eq!(artifacts.len(), 1);
        std::fs::write(out.join(&file), artifacts[0].bytes())?;
        cases.push(serde_json::json!({
            "file": file, "bases": b, "direction": c.direction, "poles": c.poles,
            "declared_current_limit_ka": limit, "physical_current_ka": c.valve_current.norm(),
            "effective_current_limit_ka": report.station_policy[0].effective_current_limit_pu * b.current_ka(),
            "objective": c.objective(), "point": point, "native_max_violation": report.max_violation,
            "expected_feasible": limit > 1.0 || b.converter_kv < 300.0,
        }));
    }
    std::fs::write(
        out.join("cases.json"),
        serde_json::to_string_pretty(&cases)? + "\n",
    )?;
    Ok(())
}
