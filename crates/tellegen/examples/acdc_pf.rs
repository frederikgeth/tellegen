//! Reproduce native/reference PF control and allocation qualification.
#[path = "../tests/support/acdc_pf.rs"]
mod fixture;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output missing")?);
    if std::env::args().nth(2).as_deref() == Some("verify") {
        return verify(&out);
    }
    std::fs::create_dir_all(&out)?;
    let mut cases = vec![];
    for (case, name) in fixture::CASES.iter().enumerate() {
        let instance = fixture::instance(case);
        let net = instance.network();
        let mut ids =
            serde_json::json!({"bus":{},"gen":{},"branch":{},"busdc":{},"branchdc":{},"convdc":{}});
        for b in net.buses() {
            ids["bus"][b.id.0.to_string()] = serde_json::json!(b.id.0.to_string());
        }
        for (i, g) in net.generators().iter().enumerate() {
            ids["gen"][g.uid.as_deref().unwrap()] = serde_json::json!((i + 1).to_string());
        }
        for (i, b) in net.branches().iter().enumerate() {
            ids["branch"][b.uid.as_deref().unwrap()] = serde_json::json!((i + 1).to_string());
        }
        let owner = net.ac_dc_equivalent().unwrap();
        for b in &owner.buses {
            ids["busdc"][b.component.to_string()] = serde_json::json!(b.external_id.to_string());
        }
        for (i, b) in owner.branches.iter().enumerate() {
            ids["branchdc"][b.component.to_string()] = serde_json::json!((i + 1).to_string());
        }
        for (i, s) in owner.stations.iter().enumerate() {
            ids["convdc"][s.component.to_string()] = serde_json::json!((i + 1).to_string());
        }
        let file = format!("pf_{case}.m");
        let output = powerio::emit(
            &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(net.clone())),
            "matpower-acdc",
            powerio::Destination::memory(&file)?,
        )?;
        let powerio::EmittedOutput::Memory { artifacts } = output.output() else {
            panic!()
        };
        std::fs::write(out.join(&file), artifacts[0].bytes())?;
        let model = PreparedAcDcPf::new(instance)?;
        let mut solves = vec![];
        let mut perturbed = model.initial_point(&AcDcPfStart::Declared)?;
        for (i, (key, value)) in perturbed.iter_mut().enumerate() {
            if key.starts_with("vm[")
                || key.ends_with("/vm")
                || key.ends_with("/vmfilt")
                || key.ends_with("/vmconv")
            {
                *value *= 1.01;
            } else if key.starts_with("va[") || key.ends_with("/vafilt") || key.ends_with("/vaconv")
            {
                *value += 0.01 * ((i % 3) as f64 - 1.);
            }
        }
        for (start, label) in [
            (AcDcPfStart::Declared, "declared"),
            (AcDcPfStart::Flat, "flat"),
            (
                AcDcPfStart::Point {
                    model_fingerprint: model.manifest().model_fingerprint.clone(),
                    values: perturbed,
                },
                "perturbed",
            ),
        ] {
            let result = model
                .solve(start, None)
                .map_err(|e| format!("{name}/{label}: {e}"))?;
            solves.push(serde_json::json!({"start":label,"result":result}));
        }
        cases.push(serde_json::json!({"case":case,"name":name,"file":file,"ids":ids,"preparation":model.manifest(),"solves":solves}));
    }
    std::fs::write(
        out.join("native.json"),
        serde_json::to_string_pretty(&cases)? + "\n",
    )?;
    println!("8 PF cases / 24 native solves pass");
    Ok(())
}
fn verify(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let r: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let mut worst: f64 = 0.;
    let mut checks = 0;
    for case in r["cases"].as_array().ok_or("reference cases missing")? {
        let model =
            PreparedAcDcPf::new(fixture::instance(case["case"].as_u64().unwrap() as usize))?;
        for s in case["solves"].as_array().unwrap() {
            let point = serde_json::from_value(s["point"].clone())?;
            let result = model.evaluate(&point)?;
            assert!(result.max_violation < 1e-7);
            worst = worst.max(result.max_violation);
            checks += 1;
        }
    }
    assert_eq!(checks, 24);
    std::fs::write(
        out.join("native-cross-check.json"),
        serde_json::to_string_pretty(&serde_json::json!({"checks":checks,"max_violation":worst}))?
            + "\n",
    )?;
    println!("24 reference PF points pass native equations and sharing");
    Ok(())
}
