//! Reproduce native/reference PF control and allocation qualification.
#[path = "../tests/support/acdc_pf_case24.rs"]
mod case24;
#[path = "../tests/support/acdc_pf_case39.rs"]
mod case39;
#[path = "../tests/support/acdc_pf_droop.rs"]
mod case67;
#[path = "../tests/support/acdc_pf.rs"]
mod fixture;
#[path = "../tests/support/acdc_pf_shared.rs"]
mod shared;
fn names() -> &'static [&'static str] {
    match std::env::var("TELLEGEN_ACDC_PF_SUITE")
        .as_deref()
        .unwrap_or("case5")
    {
        "case5" => &fixture::CASES,
        "case24" => &case24::CASES,
        "case39" => &case39::CASES,
        "case67" => &case67::CASES,
        "shared" => &shared::CASES,
        _ => panic!("unknown PF suite"),
    }
}
fn instance(case: usize) -> powerio::AcDcPfInstance {
    if names() == case24::CASES {
        case24::instance(case)
    } else if names() == case39::CASES {
        case39::instance(case)
    } else if names() == case67::CASES {
        case67::instance(case)
    } else if names() == shared::CASES {
        shared::instance(case)
    } else {
        fixture::instance(case)
    }
}
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output missing")?);
    if std::env::args().nth(2).as_deref() == Some("verify") {
        return verify(&out);
    }
    std::fs::create_dir_all(&out)?;
    if names() == case24::CASES {
        let refusals: Vec<_> = case24::refusals()
            .into_iter()
            .chain(case24::larger_refusals())
            .map(|(name, net, expected)| {
                let error = powerio::AcDcPfInstance::from_network(net, case24::PROFILE)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(expected), "{name}: {error}");
                serde_json::json!({"case":name,"error":error})
            })
            .collect();
        std::fs::write(
            out.join("refusals.json"),
            serde_json::to_string_pretty(&refusals)? + "\n",
        )?;
    }
    if names() == case39::CASES {
        let refusals: Vec<_> = case39::refusals()
            .into_iter()
            .map(|(name, net, expected)| {
                let error = powerio::AcDcPfInstance::from_network(net, case39::PROFILE)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(expected), "{name}: {error}");
                serde_json::json!({"case":name,"error":error})
            })
            .collect();
        std::fs::write(
            out.join("refusals.json"),
            serde_json::to_string_pretty(&refusals)? + "\n",
        )?;
    }
    if names() == case67::CASES {
        let refusals: Vec<_> = case67::refusals()
            .into_iter()
            .map(|(name, net, expected)| {
                let error = powerio::AcDcPfInstance::from_network(net, case67::PROFILE)
                    .unwrap_err()
                    .to_string();
                assert!(error.contains(expected), "{name}: {error}");
                serde_json::json!({"case":name,"error":error})
            })
            .collect();
        std::fs::write(
            out.join("refusals.json"),
            serde_json::to_string_pretty(&refusals)? + "\n",
        )?;
    }
    if names() == case39::CASES {
        write_control_probes(&out)?;
    }
    let mut cases = vec![];
    for (case, name) in names().iter().enumerate() {
        println!("Native PF {name}");
        let instance = instance(case);
        let net = instance.network();
        let mut ids =
            serde_json::json!({"bus":{},"gen":{},"branch":{},"busdc":{},"branchdc":{},"convdc":{}});
        for b in net.buses() {
            ids["bus"][b.id.0.to_string()] = serde_json::json!(b.id.0.to_string());
        }
        for (i, g) in net
            .generators()
            .iter()
            .enumerate()
            .filter(|(_, g)| g.in_service)
        {
            ids["gen"][g.uid.as_deref().unwrap()] = serde_json::json!((i + 1).to_string());
        }
        for (i, b) in net
            .branches()
            .iter()
            .enumerate()
            .filter(|(_, b)| b.in_service)
        {
            ids["branch"][b.uid.as_deref().unwrap()] = serde_json::json!((i + 1).to_string());
        }
        let owner = net.ac_dc_equivalent().unwrap();
        for b in &owner.buses {
            ids["busdc"][b.component.to_string()] = serde_json::json!(b.external_id.to_string());
        }
        for (i, b) in owner
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.in_service)
        {
            ids["branchdc"][b.component.to_string()] = serde_json::json!((i + 1).to_string());
        }
        for (i, s) in owner
            .stations
            .iter()
            .enumerate()
            .filter(|(_, s)| s.in_service)
        {
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
        for (start, label) in starts(&model)? {
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
    println!(
        "{} PF cases / {} native solves pass",
        names().len(),
        3 * names().len()
    );
    Ok(())
}
fn verify(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let r: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let mut worst: f64 = 0.;
    let mut checks = 0;
    for case in r["cases"].as_array().ok_or("reference cases missing")? {
        let model = PreparedAcDcPf::new(instance(case["case"].as_u64().unwrap() as usize))?;
        for s in case["solves"].as_array().unwrap() {
            if s.get("point").is_none() {
                assert!(names() == case39::CASES || names() == case67::CASES);
                continue;
            }
            let point = serde_json::from_value(s["point"].clone())?;
            let result = model.evaluate(&point)?;
            assert!(result.max_violation < 1e-7);
            worst = worst.max(result.max_violation);
            checks += 1;
        }
    }
    let expected = r["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["solves"].as_array().unwrap())
        .filter(|s| s.get("point").is_some())
        .count();
    assert_eq!(checks, expected);
    assert!(checks >= names().len() * 2);
    std::fs::write(
        out.join("native-cross-check.json"),
        serde_json::to_string_pretty(&serde_json::json!({"checks":checks,"max_violation":worst}))?
            + "\n",
    )?;
    println!("{checks} reference PF points pass native equations and sharing");
    Ok(())
}

fn starts(model: &PreparedAcDcPf) -> Result<Vec<(AcDcPfStart, &'static str)>, String> {
    let mut perturbed = model.initial_point(&AcDcPfStart::Declared)?;
    for (i, (key, value)) in perturbed.iter_mut().enumerate() {
        if key.starts_with("vm[")
            || key.ends_with("/vm")
            || key.ends_with("/vmfilt")
            || key.ends_with("/vmconv")
        {
            *value *= 1.01;
        } else if key.starts_with("va[") || key.ends_with("/vafilt") || key.ends_with("/vaconv") {
            *value += 0.01 * ((i % 3) as f64 - 1.);
        }
    }
    Ok(vec![
        (AcDcPfStart::Declared, "declared"),
        (AcDcPfStart::Flat, "flat"),
        (
            AcDcPfStart::Point {
                model_fingerprint: model.manifest().model_fingerprint.clone(),
                values: perturbed,
            },
            "perturbed",
        ),
    ])
}
fn write_control_probes(out: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut probes = vec![];
    for case in [0, 1] {
        let model = PreparedAcDcPf::new(powerio::AcDcPfInstance::from_network(
            case39::control_only_network(case),
            case39::PROFILE,
        )?)?;
        let mut trials = vec![];
        for (start, label) in starts(&model)? {
            let initial = model.initial_point(&start)?;
            let result = match model.solve(start, None) {
                Ok(r) => {
                    serde_json::json!({"termination":"CONVERGED","point":r.point,"max_violation":r.validation.max_violation})
                }
                Err(e) => serde_json::json!({"termination":"FAILED_LOCAL_TRIAL","error":e}),
            };
            trials.push(serde_json::json!({"start":label,"initial_point":initial,"native":result}));
        }
        probes.push(serde_json::json!({"selected_converter":if case==0 {1}else{8},"preparation":model.manifest(),"trials":trials}));
    }
    std::fs::write(
        out.join("control-only-native.json"),
        serde_json::to_string_pretty(&probes)? + "\n",
    )?;
    Ok(())
}
