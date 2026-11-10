//! Export physical branch-loss witnesses; this does not solve directional OPF.
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
            let model = case.equations(b);
            let r = model.evaluate(&point)?;
            let expected = case.expected_loss_residual_mw().abs() / b.mva;
            for e in &r.equations {
                assert!(
                    (e.violation
                        - if e.name == format!("station/{STATION}/loss") {
                            expected
                        } else {
                            0.
                        })
                    .abs()
                        < 1e-9,
                    "{}: {}",
                    e.name,
                    e.violation
                );
            }
            assert!((r.max_violation - expected).abs() < 1e-9);
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
            records.push(serde_json::json!({"file":file,"case":case,"bases":b,"point":point,"named_mode":case.named_mode(),"native_max_violation":r.max_violation,"source_loss_residual_mw":case.expected_loss_residual_mw(),"current_ka":c.valve_current.norm(),"pcc_mw":c.pcc_power.re,"valve_mw":c.valve_power.re,"dc_withdrawal_mw":-c.dc_from_mw,"selected_loss_mw":c.valve_loss_mw,"objective":r.objective}));
        }
    }
    assert_eq!(records.len(), 1728);
    std::fs::write(
        out.join("cases.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    println!(
        "1728 physical loss witnesses; native residuals match the analytic coefficient differences"
    );
    Ok(())
}
