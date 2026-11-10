//! Export independently constructed physical circuits for the pinned Julia checker.
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use physical::*;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output directory")?);
    let selection = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "compatibility".into());
    let profile = match selection.as_str() {
        "compatibility" => powerio::AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
        "declared-current" => powerio::AcDcOpfProfile::DeclaredCurrentAcpVscV1,
        _ => return Err("profile must be compatibility or declared-current".into()),
    };
    let declared = selection == "declared-current";
    std::fs::create_dir_all(&out)?;
    let mut cases = Vec::new();
    let mut inputs = Vec::new();
    for direction in [-1.0, 1.0] {
        for poles in [1, 2] {
            for b in bases() {
                let limits = if declared {
                    vec![
                        0.05,
                        Circuit::new(direction, poles).valve_current.norm(),
                        0.3,
                    ]
                } else {
                    vec![5.0]
                };
                for limit in limits {
                    inputs.push((Circuit::new(direction, poles), b, limit));
                }
            }
        }
    }
    if !declared {
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
    }
    for (index, (c, b, limit)) in inputs.into_iter().enumerate() {
        let net = c.network(b, limit);
        let point = c.point(b);
        let intent = powerio::AcDcOpfInstance::from_network(net.clone(), profile)?;
        let report =
            ReferenceEquations::from_instance(&intent, EquationMode::Opf)?.evaluate(&point)?;
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
            "file": file, "profile": selection, "bases": b, "direction": c.direction, "poles": c.poles,
            "declared_current_limit_ka": limit, "physical_current_ka": c.valve_current.norm(),
            "effective_current_limit_ka": report.station_policy[0].effective_current_limit_pu * b.current_ka(),
            "objective": c.objective(), "point": point, "native_max_violation": report.max_violation,
            "expected_feasible": if declared { limit >= c.valve_current.norm() } else { limit > 1.0 || b.converter_kv < 300.0 },
        }));
    }
    std::fs::write(
        out.join("cases.json"),
        serde_json::to_string_pretty(&cases)? + "\n",
    )?;
    Ok(())
}
