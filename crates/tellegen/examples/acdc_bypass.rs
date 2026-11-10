//! Fresh exports of bypass/unequal-base physical qualification cases.
#[path = "../tests/support/acdc_bypass.rs"]
mod bypass;
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use powerio::{AcDcOpfInstance, AcDcOpfProfile};
use tellegen::ac_dc::{EquationMode, ReferenceEquations};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output directory")?);
    std::fs::create_dir_all(&out)?;
    let mut records = Vec::new();
    for (i, case) in bypass::cases().into_iter().enumerate() {
        let (net, point) = case.data();
        let mut native = Vec::new();
        for profile in [
            AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
            AcDcOpfProfile::DeclaredCurrentAcpVscV1,
        ] {
            let intent = AcDcOpfInstance::from_network(net.clone(), profile)?;
            let r =
                ReferenceEquations::from_instance(&intent, EquationMode::Opf)?.evaluate(&point)?;
            let actual: std::collections::BTreeSet<_> = r
                .equations
                .iter()
                .chain(&r.bounds)
                .filter(|v| v.violation > 1e-9)
                .map(|v| v.name.clone())
                .collect();
            assert_eq!(actual, case.expected_violations().into_iter().collect());
            native.push(r);
        }
        assert!((native[0].max_violation - native[1].max_violation).abs() < 1e-12);
        let file = format!("bypass_{i:03}.m");
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
        let c = &case.circuit;
        records.push(serde_json::json!({"file":file,"profile":"compatibility","suite":"bypass","kind":case.kind,
            "bases":case.bases,"dc_to_base_kv":case.dc_to_base_kv,"elements":c.elements,
            "direction":c.direction,"poles":c.poles,"point":point,
            "declared_current_limit_ka":5.0,"effective_current_limit_ka":5.0,"physical_current_ka":c.valve_current.norm(),
            "objective":c.objective(),"native_max_violation":native[0].max_violation,
            "expected_feasible":case.kind=="qualified","expected_violations":case.expected_violations(),
            "native_violations":native[0].equations.iter().filter(|v|v.violation>1e-9).map(|v|(&v.name,v.violation)).collect::<std::collections::BTreeMap<_,_>>()
        }));
    }
    std::fs::write(
        out.join("cases.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    Ok(())
}
