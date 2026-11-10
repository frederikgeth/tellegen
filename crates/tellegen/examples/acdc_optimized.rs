//! Solve explicitly equivalent physical fixtures and verify independent Julia solutions.
#[path = "../tests/support/acdc_optimized.rs"]
mod optimized;
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use optimized::*;
use physical::*;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output")?);
    let verify = std::env::args().nth(2).as_deref() == Some("verify");
    if verify {
        return verify_reference(&out);
    }
    std::fs::create_dir_all(&out)?;
    let mut records = Vec::new();
    for mask in 0..8 {
        for direction in [-1., 1.] {
            for poles in [1, 2] {
                let c = Circuit::with_elements(direction, poles, Elements::from_mask(mask));
                let group = format!("mask{mask}/direction{direction}/poles{poles}");
                let mut group_solutions = Vec::new();
                for (index, b) in coordinates().into_iter().enumerate() {
                    let model = prepared(&c, b);
                    let mut solves = Vec::new();
                    for (name, x) in starts(&c, b) {
                        let result = model
                            .solve(start(&model, x.clone()), None)
                            .map_err(|e| format!("{group}/base{index}/{name}: {e}"))?;
                        let physical = b.physical(&result.point);
                        let physical_error = physical_residual(&c, &physical);
                        assert!(physical_error < 1e-6);
                        cost_close(result.validation.objective, cost(&physical));
                        for (other, _) in &group_solutions {
                            cost_close(result.validation.objective, *other);
                        }
                        group_solutions.push((result.validation.objective, physical.clone()));
                        solves.push(serde_json::json!({"start":name,"initial_point":x,"point":result.point,"physical":physical,
                    "objective":result.validation.objective,"max_violation":result.validation.max_violation,
                    "physical_residual":physical_error,"kkt":result.solver_kkt_error,"iterations":result.iterations}));
                    }
                    let file = format!("optimized_{:03}.m", records.len());
                    let emitted = powerio::emit(
                        &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(network(
                            &c, b,
                        ))),
                        "matpower-acdc",
                        powerio::Destination::memory(&file)?,
                    )?;
                    let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
                        panic!()
                    };
                    assert_eq!(artifacts.len(), 1);
                    std::fs::write(out.join(&file), artifacts[0].bytes())?;
                    records.push(serde_json::json!({"file":file,"group":group,"mask":mask,"direction":direction,"poles":poles,
                "base_index":index,"bases":b,"fingerprint":model.manifest().model_fingerprint,
                "physical_bounds":physical_bounds(&model,b),"powerio_revision":model.manifest().powerio_revision,"solves":solves}));
                }
                // Cross-evaluate all native solved physical points on every equivalent base.
                for b in coordinates() {
                    let target = prepared(&c, b);
                    for (objective, point) in &group_solutions {
                        let r = target.evaluate(&b.per_unit(point))?;
                        assert!(
                            r.max_violation < 1e-7,
                            "{group}: rebased violation {}",
                            r.max_violation
                        );
                        cost_close(r.objective, *objective);
                    }
                }
            }
        }
    }
    std::fs::write(
        out.join("native.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    println!(
        "{} networks / {} native solves; equivalent-base feasibility and costs pass",
        records.len(),
        records.len() * 2
    );
    Ok(())
}
fn verify_reference(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let records: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let mut checks = 0;
    let mut worst: f64 = 0.;
    let mut physical_worst: f64 = 0.;
    for record in records["records"].as_array().ok_or("missing records")? {
        let c = Circuit::with_elements(
            record["direction"].as_f64().unwrap(),
            record["poles"].as_u64().unwrap() as u8,
            Elements::from_mask(record["mask"].as_u64().unwrap() as u8),
        );
        let b: Coordinates = serde_json::from_value(record["bases"].clone())?;
        for solve in record["solves"].as_array().unwrap() {
            let point: Point = serde_json::from_value(solve["point"].clone())?;
            let physical = b.physical(&point);
            let residual = physical_residual(&c, &physical);
            assert!(residual < 1e-6);
            physical_worst = physical_worst.max(residual);
            let objective = solve["objective"].as_f64().unwrap();
            for target in coordinates() {
                let model = prepared(&c, target);
                let r = model.evaluate(&target.per_unit(&physical))?;
                assert!(
                    r.max_violation < 1e-7,
                    "reference fails native: {}",
                    r.max_violation
                );
                cost_close(r.objective, objective);
                worst = worst.max(r.max_violation);
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 1024);
    std::fs::write(
        out.join("native-cross-check.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"checks":checks,"max_violation":worst,"max_physical_residual":physical_worst}),
        )? + "\n",
    )?;
    println!("{checks} reference solutions checked on native bases; max {worst}");
    Ok(())
}
