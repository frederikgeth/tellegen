//! Physical-coordinate PF and independent reference cross-validation.
#[path = "../tests/support/acdc_pf_fixed.rs"]
mod fixture;
#[path = "../tests/support/acdc_optimized.rs"]
mod optimized;
#[path = "../tests/support/acdc_pf_physical.rs"]
mod pf_physical;
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use optimized::{coordinates, Coordinates, Point};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output")?);
    if std::env::args().nth(2).as_deref() == Some("verify") {
        return verify(&out);
    }
    std::fs::create_dir_all(&out)?;
    let mut records = Vec::new();
    for case in fixture::cases() {
        let c = case.circuit();
        let group = case.group();
        let mut points = Vec::new();
        for (index, b) in coordinates().into_iter().enumerate() {
            let model = case.prepared(b);
            let mut solves = Vec::new();
            for (name, start) in pf_physical::starts(&c, b, &model) {
                let r = model
                    .solve(start, None)
                    .map_err(|e| format!("{group}/base{index}/{name}: {e}"))?;
                let residual = case.check(b, &r.point);
                points.push(b.physical(&r.point));
                solves.push(serde_json::json!({"start":name,"initial_point":r.initial_point,"point":r.point,"physical":b.physical(&r.point),"max_violation":r.validation.max_violation,"physical_residual":residual,"iterations":r.iterations,"limit_violations":r.limit_violations}));
            }
            let file = format!("fixed_pf_{:03}.m", records.len());
            let emitted = powerio::emit(
                &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(case.network(b))),
                "matpower-acdc",
                powerio::Destination::memory(&file)?,
            )?;
            let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
                panic!()
            };
            assert_eq!(artifacts.len(), 1);
            std::fs::write(out.join(&file), artifacts[0].bytes())?;
            records.push(serde_json::json!({"file":file,"group":group,"case":case,"mask":case.mask,"droop":case.droop,"bases":b,"base_index":index,"loss_modes":case.modes(),"fingerprint":model.manifest().model_fingerprint,"powerio_revision":model.manifest().powerio_revision,"solves":solves}));
        }
        for b in coordinates() {
            let m = case.prepared(b);
            for p in &points {
                assert!(m.evaluate(&b.per_unit(p))?.max_violation < 1e-7);
            }
        }
    }
    std::fs::write(
        out.join("native.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    println!(
        "{} representations / {} native solves",
        records.len(),
        records.len() * 2
    );
    Ok(())
}
fn verify(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let records: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let (mut checks, mut worst, mut physical_worst) = (0, 0_f64, 0_f64);
    for r in records["records"].as_array().ok_or("missing records")? {
        let case: fixture::Case = serde_json::from_value(r["case"].clone())?;
        let b: Coordinates = serde_json::from_value(r["bases"].clone())?;
        for s in r["solves"].as_array().unwrap() {
            let p: Point = serde_json::from_value(s["point"].clone())?;
            physical_worst = physical_worst.max(case.check(b, &p));
            for target in coordinates() {
                let e = case
                    .prepared(target)
                    .evaluate(&target.per_unit(&b.physical(&p)))?;
                assert!(e.max_violation < 1e-7);
                worst = worst.max(e.max_violation);
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 2304);
    std::fs::write(
        out.join("native-cross-check.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"checks":checks,"max_violation":worst,"max_physical_residual":physical_worst}),
        )? + "\n",
    )?;
    println!("{checks} reference points checked on native bases; max {worst}");
    Ok(())
}
