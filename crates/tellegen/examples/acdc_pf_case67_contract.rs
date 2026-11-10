//! Reference-only case67 PF contract study with independent native residual checks.
#[path = "../tests/support/acdc_pf_case67_contract.rs"]
mod fixture;
use powerio::{AcDcOpfInstance, AcDcOpfProfile, BalancedNetwork, BusType};
use powerio_tx::acdc::{AcConverterControl, DcConverterControl};
use std::{collections::BTreeMap, path::Path};
use tellegen::ac_dc::{EquationMode, ReferenceEquations};
type Point = BTreeMap<String, f64>;
fn equations(n: BalancedNetwork) -> ReferenceEquations {
    ReferenceEquations::from_instance(
        &AcDcOpfInstance::from_network(n, AcDcOpfProfile::DeclaredCurrentAcpVscV1).unwrap(),
        EquationMode::Opf,
    )
    .unwrap()
}
fn controls(n: &BalancedNetwork, x: &Point) -> Point {
    let mut rows = Point::new();
    let base = n.base_mva();
    for b in n.buses() {
        let gens: Vec<_> = n
            .generators()
            .iter()
            .filter(|g| g.in_service && g.bus == b.id)
            .collect();
        assert!(
            gens.len() <= 1,
            "contract fixture has one generator per bus"
        );
        if b.kind == BusType::Ref {
            rows.insert(format!("angle/{}", b.id.0), x[&format!("va[{}]", b.id.0)]);
        }
        for g in gens {
            let id = g.uid.as_deref().unwrap();
            rows.insert(
                format!("vm/{}", b.id.0),
                x[&format!("vm[{}]", b.id.0)] - b.vm,
            );
            if b.kind != BusType::Ref {
                rows.insert(format!("pg/{id}"), x[&format!("pg[{id}]")] - g.pg / base);
            }
        }
    }
    for s in n
        .ac_dc_equivalent()
        .unwrap()
        .stations
        .iter()
        .filter(|s| s.in_service)
    {
        let id = s.component.to_string();
        let p = |f| x[&format!("station/{id}/{f}")];
        let vm = x[&format!("dc_bus/{}/vm", s.dc_bus)];
        let set = &s.settings;
        let r = match s.dc_control {
            DcConverterControl::ActivePowerAtPcc => p("pgrid") - set.pcc_withdrawal_mw / base,
            DcConverterControl::DcVoltage => vm - set.dc_voltage_target_pu,
            DcConverterControl::DcPowerDroop => fixture::droop_residual(
                p("pdc"),
                vm,
                set.dc_withdrawal_target_mw,
                set.dc_voltage_target_pu,
                set.droop,
                base,
            ),
            _ => panic!("outside contract"),
        };
        rows.insert(format!("dc/{id}"), r);
        if s.ac_control == AcConverterControl::ReactivePowerAtPcc {
            rows.insert(
                format!("q/{id}"),
                p("qgrid") - set.pcc_withdrawal_mvar / base,
            );
        } else {
            let b = n.buses().iter().find(|b| b.id == s.ac_bus).unwrap();
            rows.insert(format!("vac/{id}"), x[&format!("vm[{}]", b.id.0)] - b.vm);
            let g = n
                .generators()
                .iter()
                .find(|g| g.in_service && g.bus == b.id)
                .unwrap();
            rows.insert(
                format!("shared_q/{id}"),
                x[&format!("qg[{}]", g.uid.as_deref().unwrap())] + p("qgrid")
                    - (g.qg + set.pcc_withdrawal_mvar) / base,
            );
        }
    }
    rows
}
fn export(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc/case67");
    let map: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("identity-map.json"))?)?;
    let mut cases = vec![];
    for (case, name) in fixture::CASES.iter().enumerate() {
        let n = fixture::network(case);
        let model = equations(n.clone());
        let mut ids = map.clone();
        for b in n
            .ac_dc_equivalent()
            .unwrap()
            .branches
            .iter()
            .filter(|b| !b.in_service)
        {
            ids["branchdc"]
                .as_object_mut()
                .unwrap()
                .remove(&b.component.to_string());
        }
        let file = format!("case67_contract_{case}.m");
        let e = powerio::emit(
            &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(n.clone())),
            "matpower-acdc",
            powerio::Destination::memory(&file)?,
        )?;
        let powerio::EmittedOutput::Memory { artifacts } = e.output() else {
            panic!()
        };
        std::fs::write(out.join(&file), artifacts[0].bytes())?;
        let flat: Point = model
            .variable_names()
            .iter()
            .map(|k| {
                (
                    k.clone(),
                    if k.starts_with("vm[")
                        || k.ends_with("/vm")
                        || k.ends_with("/vmfilt")
                        || k.ends_with("/vmconv")
                    {
                        1.
                    } else if k.ends_with("/iconv") {
                        0.1
                    } else {
                        0.
                    },
                )
            })
            .collect();
        let mut declared = flat.clone();
        for b in n.buses() {
            declared.insert(format!("vm[{}]", b.id.0), b.vm);
            declared.insert(format!("va[{}]", b.id.0), b.va.to_radians());
        }
        for g in n.generators().iter().filter(|g| g.in_service) {
            for (f, v) in [("pg", g.pg), ("qg", g.qg)] {
                declared.insert(
                    format!("{f}[{}]", g.uid.as_deref().unwrap()),
                    v / n.base_mva(),
                );
            }
        }
        for s in &n.ac_dc_equivalent().unwrap().stations {
            let pg = s.settings.pcc_withdrawal_mw / n.base_mva();
            let qg = s.settings.pcc_withdrawal_mvar / n.base_mva();
            for (f, v) in [
                ("pgrid", pg),
                ("qgrid", qg),
                ("ptf_to", -pg),
                ("qtf_to", -qg),
                ("ppr_fr", pg),
                ("qpr_fr", qg),
                ("pconv", pg),
                ("qconv", qg),
                ("pdc", -pg),
                ("iconv", pg.hypot(qg).max(0.1)),
            ] {
                declared.insert(format!("station/{}/{f}", s.component), v);
            }
        }
        let mut perturbed = declared.clone();
        for (i, (k, v)) in perturbed.iter_mut().enumerate() {
            if k.starts_with("vm[")
                || k.ends_with("/vm")
                || k.ends_with("/vmfilt")
                || k.ends_with("/vmconv")
            {
                *v *= 1.01;
            } else if k.starts_with("va[") || k.ends_with("/vafilt") || k.ends_with("/vaconv") {
                *v += 0.01 * ((i % 3) as f64 - 1.);
            }
        }
        cases.push(serde_json::json!({"case":case,"name":name,"file":file,"ids":ids,"starts":{"declared":declared,"flat":flat,"perturbed":perturbed}}));
    }
    std::fs::write(
        out.join("inputs.json"),
        serde_json::to_string_pretty(&cases)? + "\n",
    )?;
    Ok(())
}
fn verify(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let reference: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("reference.json"))?)?;
    let mut reports = vec![];
    for c in reference["cases"].as_array().unwrap() {
        let n = fixture::network(c["case"].as_u64().unwrap() as usize);
        let model = equations(n.clone());
        for s in c["solves"].as_array().unwrap() {
            if s.get("native_point").is_none() {
                continue;
            }
            let point: Point = serde_json::from_value(s["native_point"].clone())?;
            let report = model.evaluate(&point)?;
            let electrical = report
                .equations
                .iter()
                .filter(|r| r.lower == r.upper)
                .map(|r| r.violation)
                .fold(0., f64::max);
            assert!(electrical < 1e-7, "{} electrical {electrical}", c["name"]);
            let rows = controls(&n, &point);
            let control = rows.values().map(|v| v.abs()).fold(0., f64::max);
            assert!(control < 1e-7, "{} controls {rows:?}", c["name"]);
            let limits = report
                .bounds
                .iter()
                .chain(report.equations.iter().filter(|r| r.lower != r.upper))
                .filter(|r| r.violation > 1e-7)
                .count();
            let mut negative = 0;
            for st in n
                .ac_dc_equivalent()
                .unwrap()
                .stations
                .iter()
                .filter(|s| s.in_service && s.dc_control == DcConverterControl::DcPowerDroop)
            {
                for (key, delta) in [
                    (format!("station/{}/pdc", st.component), 0.01),
                    (format!("dc_bus/{}/vm", st.dc_bus), 0.001),
                ] {
                    let mut bad = point.clone();
                    *bad.get_mut(&key).unwrap() += delta;
                    assert!(controls(&n, &bad)[&format!("dc/{}", st.component)].abs() > 0.009);
                    negative += 1;
                }
            }
            let g = n.generators().iter().find(|g| g.bus.0 == 67).unwrap();
            let pg67 = point[&format!("pg[{}]", g.uid.as_deref().unwrap())] * n.base_mva();
            assert!(
                (pg67
                    - n.ac_dc_equivalent().unwrap().stations[8]
                        .settings
                        .pcc_withdrawal_mw)
                    .abs()
                    < 1e-7
            );
            reports.push(serde_json::json!({"case":c["name"],"start":s["start"],"max_native_electrical_residual":electrical,"max_control_residual":control,"control_rows":rows.len(),"negative_controls":negative,"rating_violations":limits,"generator_67_mw":pg67}));
        }
    }
    let expected = reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["solves"].as_array().unwrap())
        .filter(|s| s.get("native_point").is_some())
        .count();
    assert_eq!(reports.len(), expected);
    assert!(reports.len() >= fixture::CASES.len() * 2);
    std::fs::write(
        out.join("native-checks.json"),
        serde_json::to_string_pretty(&reports)? + "\n",
    )?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(std::env::args().nth(1).ok_or("output missing")?);
    std::fs::create_dir_all(&out)?;
    if std::env::args().nth(2).as_deref() == Some("verify") {
        verify(&out)
    } else {
        export(&out)
    }
}
