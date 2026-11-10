//! Check fixed-mode sign domains at idle, zero and near-zero valve power.
#[path = "../tests/support/acdc_losses.rs"]
mod losses;
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use losses::*;
use physical::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output")?);
    std::fs::create_dir_all(&out)?;
    let mut records = vec![];
    for case in cases() {
        for b in bases() {
            let c = case.circuit();
            let point = c.point(b);
            let instance = powerio::AcDcOpfInstance::from_network_with_loss_modes(
                case.network(b),
                powerio::AcDcOpfProfile::FixedModePhysicalAcpVscV1,
                vec![powerio::AcDcStationLossMode {
                    component: STATION.into(),
                    mode: if case.selected == "rectifier" {
                        powerio::AcDcValveLossMode::Rectifier
                    } else {
                        powerio::AcDcValveLossMode::Inverter
                    },
                }],
            )?;
            let model = tellegen::ac_dc::ReferenceEquations::from_instance(
                &instance,
                tellegen::ac_dc::EquationMode::Opf,
            )?;
            let r = model.evaluate(&point)?;
            assert!(r.equations.iter().all(|e| e.violation < 1e-9));
            let wrong = if case.selected == "rectifier" {
                (-c.valve_power.re).max(0.)
            } else {
                c.valve_power.re.max(0.)
            };
            let bound = r
                .bounds
                .iter()
                .find(|v| v.name == format!("station/{STATION}/pconv"))
                .unwrap();
            assert!((bound.violation * b.mva - wrong).abs() < 1e-12);
            if wrong > 0. {
                assert!(bound.violation > 0.);
            } else {
                assert!(r.max_violation < 1e-9);
            }
            let file = format!("loss_{:04}.m", records.len());
            let emitted = powerio::emit(
                &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(case.network(b))),
                "matpower-acdc",
                powerio::Destination::memory(&file)?,
            )?;
            let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
                panic!()
            };
            std::fs::write(out.join(&file), artifacts[0].bytes())?;
            records.push(serde_json::json!({"fixed_mode":true,"sign_violation_mw":wrong,"file":file,"case":case,"bases":b,"point":point,"named_mode":case.named_mode(),"native_max_violation":r.max_violation,"source_loss_residual_mw":case.expected_loss_residual_mw(),"current_ka":c.valve_current.norm(),"pcc_mw":c.pcc_power.re,"valve_mw":c.valve_power.re,"dc_withdrawal_mw":-c.dc_from_mw,"selected_loss_mw":c.valve_loss_mw,"objective":r.objective}));
        }
    }
    assert_eq!(records.len(), 1728);
    std::fs::write(
        out.join("cases.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    println!("1728 fixed-mode boundary witnesses; loss equations and valve sign domains checked");
    Ok(())
}
