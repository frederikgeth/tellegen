//! Independent optimization qualification for multi-terminal DC mesh fixtures.
#[path = "../tests/support/acdc_mesh.rs"]
mod mesh;
#[path = "../tests/support/acdc_optimized.rs"]
mod optimized;
#[path = "../tests/support/acdc_physical.rs"]
mod physical;
use mesh::*;
use optimized::{cost_close, start, Point};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("missing output")?);
    if std::env::args().nth(2).as_deref() == Some("verify") {
        return verify(&out);
    }
    std::fs::create_dir_all(&out)?;
    let mut records = vec![];
    let mut cross_checks = 0;
    let mut cross_worst: f64 = 0.;
    for case in cases() {
        let mut points = vec![];
        for (base_index, b) in bases().into_iter().enumerate() {
            let model = case.prepared(b);
            assert_eq!(model.manifest().variables.len(), 63);
            assert_eq!(model.manifest().angle_references.len(), 3);
            let mut solves = vec![];
            for (name, initial) in case.starts(b) {
                let r = model
                    .solve(start(&model, initial.clone()), None)
                    .map_err(|e| format!("{}/base{base_index}/{name}: {e}", case.group()))?;
                let physical = b.physical(&r.point);
                let error = physical_residual(case, &physical);
                assert!(error < 1e-6, "physical {error}");
                binding(case, &physical);
                cost_close(case.cost(&physical), r.validation.objective);
                for (obj, _) in &points {
                    cost_close(*obj, r.validation.objective)
                }
                points.push((r.validation.objective, physical.clone()));
                solves.push(serde_json::json!({"start":name,"initial_point":initial,"point":r.point,"physical":physical,"objective":r.validation.objective,"max_violation":r.validation.max_violation,"physical_residual":error,"kkt":r.solver_kkt_error,"iterations":r.iterations}));
            }
            let file = format!("mesh_{:03}.m", records.len());
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
            records.push(serde_json::json!({"file":file,"case":case,"group":case.group(),"base_index":base_index,"bases":b,"fingerprint":model.manifest().model_fingerprint,"physical_bounds":bounds(&model,b),"powerio_revision":model.manifest().powerio_revision,"solves":solves}));
        }
        for b in bases() {
            let model = case.prepared(b);
            for (obj, x) in &points {
                let r = model.evaluate(&b.per_unit(x))?;
                assert!(r.max_violation < 1e-7);
                cost_close(*obj, r.objective);
                cross_worst = cross_worst.max(r.max_violation);
                cross_checks += 1;
            }
        }
    }
    assert_eq!(records.len(), 96);
    assert_eq!(cross_checks, 768);
    std::fs::write(
        out.join("native.json"),
        serde_json::to_string_pretty(&records)? + "\n",
    )?;
    std::fs::write(
        out.join("native-base-check.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"checks":cross_checks,"max_violation":cross_worst}),
        )? + "\n",
    )?;
    println!("96 representations, 192 native solves; 768 base checks pass");
    Ok(())
}
fn verify(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let mut checks = 0;
    let mut worst: f64 = 0.;
    let mut physical_worst: f64 = 0.;
    for r in data["records"].as_array().ok_or("records missing")? {
        let case: Case = serde_json::from_value(r["case"].clone())?;
        let b: Bases3 = serde_json::from_value(r["bases"].clone())?;
        for solve in r["solves"].as_array().unwrap() {
            let point: Point = serde_json::from_value(solve["point"].clone())?;
            let x = b.physical(&point);
            let error = physical_residual(case, &x);
            assert!(error < 1e-6);
            physical_worst = physical_worst.max(error);
            binding(case, &x);
            for target in bases() {
                let model = case.prepared(target);
                let r = model.evaluate(&target.per_unit(&x))?;
                assert!(r.max_violation < 1e-7);
                cost_close(r.objective, solve["objective"].as_f64().unwrap());
                worst = worst.max(r.max_violation);
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 768);
    std::fs::write(
        out.join("native-cross-check.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"checks":checks,"max_violation":worst,"max_physical_residual":physical_worst}),
        )? + "\n",
    )?;
    println!("{checks} reference-in-native checks pass");
    Ok(())
}
