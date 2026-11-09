#![cfg(feature = "acopf")]
#[path = "support/acdc_reference.rs"]
mod reference;
use powerio::{AcOpfInstance, AcPfInstance, BalancedNetwork};
use powerio_tx::acdc::{BalancedAcDcEquipment, ConverterTechnology, DcConverterControl};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

fn fixture() -> (BalancedNetwork, Value, Value) {
    reference::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc"))
}
fn mutated(
    net: &BalancedNetwork,
    change: impl FnOnce(&mut BalancedAcDcEquipment),
) -> BalancedNetwork {
    let mut e = net.ac_dc_equivalent().unwrap().clone();
    change(&mut e);
    let mut n = net.clone();
    n.set_ac_dc_equivalent(Some(e)).unwrap();
    n
}
fn check(net: &BalancedNetwork, mode: EquationMode, x: &BTreeMap<String, f64>, expected_row: &str) {
    let report = ReferenceEquations::powermodels_acdc_0101(net, mode)
        .unwrap()
        .evaluate(x)
        .unwrap();
    assert!(
        report
            .equations
            .iter()
            .chain(&report.bounds)
            .any(|r| r.name.contains(expected_row) && r.violation > 1e-6),
        "did not detect {expected_row}: {report:?}"
    );
}

#[test]
fn case5_reference_pf_and_opf_satisfy_native_equations() {
    let (net, oracle, map) = fixture();
    let original = net.ac_dc_equivalent().unwrap().clone();
    for (label, mode, equations, bounds) in [
        ("pf", EquationMode::Pf, 94, 3),
        ("opf", EquationMode::Opf, 98, 66),
    ] {
        assert_eq!(
            oracle["solves"][label]["termination_status"],
            "LOCALLY_SOLVED"
        );
        assert!(
            oracle["solves"][label]["max_primal_violation"]
                .as_f64()
                .unwrap()
                < 1e-7
        );
        let model = ReferenceEquations::powermodels_acdc_0101(&net, mode).unwrap();
        let point = reference::point(&net, &oracle["solves"][label]["solution"], &map);
        assert_eq!(model.variable_names().len(), 95);
        let report = model.evaluate(&point).unwrap();
        assert_eq!(
            (report.equations.len(), report.bounds.len()),
            (equations, bounds)
        );
        assert!(report.max_violation < 1e-7, "{report:?}");
        if mode == EquationMode::Opf {
            assert!(
                (report.objective - oracle["solves"][label]["objective"].as_f64().unwrap()).abs()
                    < 1e-7
            );
        }
        for policy in &report.station_policy {
            let id = map["convdc"][&policy.component].as_str().unwrap();
            let effective = &oracle["effective_acdc"]["convdc"][id];
            assert_eq!(
                (policy.converter_base_kv, policy.pcc_base_kv),
                (345.0, 230.0)
            );
            assert!(policy.effective_current_limit_pu > policy.declared_current_limit_pu);
            assert!(
                (policy.effective_current_limit_pu - effective["Imax"].as_f64().unwrap()).abs()
                    < 1e-12
            );
            for (i, key) in ["LossA", "LossB", "LossCinv"].into_iter().enumerate() {
                assert!(
                    (policy.loss_coefficients_pu[i] - effective[key].as_f64().unwrap()).abs()
                        < 1e-12
                );
            }
        }
    }
    assert_eq!(net.ac_dc_equivalent().unwrap(), &original);
    // The diagnostic model cannot enable accidental AC-only solves.
    assert!(AcOpfInstance::from_network(net.clone()).is_err());
    assert!(AcPfInstance::from_network(net).is_err());
}

#[test]
fn independent_oracle_provenance_is_pinned() {
    let (_, oracle, _) = fixture();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc");
    for (file, key) in [
        ("case5_3_he.m", "source_sha256"),
        ("Manifest.toml", "manifest_sha256"),
    ] {
        let digest = Sha256::digest(std::fs::read(dir.join(file)).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(digest, oracle[key].as_str().unwrap());
    }
    assert_eq!(
        oracle["powermodelsacdc_commit"],
        "9ada4082ac087aed649f48fb5f977680b0fe06c7"
    );
}

#[test]
fn sign_pole_base_loss_filter_and_setpoint_mutations_are_detected() {
    let (net, oracle, map) = fixture();
    let pf = reference::point(&net, &oracle["solves"]["pf"]["solution"], &map);
    for (field, row) in [
        ("pgrid", "p_balance"),
        ("qgrid", "q_balance"),
        ("pconv", "reactor/pt"),
        ("qconv", "reactor/qt"),
        ("pdc", "/balance"),
    ] {
        let mut x = pf.clone();
        *x.get_mut(&format!("station/ac_dc_converter_station/1/{field}"))
            .unwrap() *= -1.0;
        check(&net, EquationMode::Pf, &x, row);
    }
    let cases: Vec<(BalancedNetwork, &str)> = vec![
        (mutated(&net, |e| e.pole_count = 1), "dc_branch/"),
        (mutated(&net, |e| e.stations[0].ac_base_kv = 230.0), "/loss"),
        (
            mutated(&net, |e| e.stations[0].losses.linear_kv *= 3.0_f64.sqrt()),
            "/loss",
        ),
        (
            mutated(&net, |e| e.stations[0].losses.inverter_quadratic_ohm *= 3.0),
            "/loss",
        ),
        (
            mutated(&net, |e| e.stations[0].losses.constant_mw = 0.0),
            "/loss",
        ),
        (
            mutated(&net, |e| e.stations[0].filter_susceptance_pu *= -1.0),
            "/filter_q",
        ),
        (
            mutated(&net, |e| e.stations[0].transformer_tap_ratio = 1.1),
            "/transformer/",
        ),
        (
            mutated(&net, |e| e.stations[0].settings.pcc_withdrawal_mw *= -1.0),
            "/p_setpoint",
        ),
        (
            mutated(&net, |e| e.stations[0].settings.pcc_withdrawal_mvar *= -1.0),
            "/q_setpoint",
        ),
        (
            mutated(&net, |e| {
                e.stations[1].settings.dc_voltage_target_pu += 0.01
            }),
            "/vdc_setpoint",
        ),
        (
            mutated(&net, |e| e.buses[0].withdrawal_mw = 1.0),
            "dc_bus/dc_equivalent_bus/1/balance",
        ),
    ];
    for (n, row) in cases {
        check(&n, EquationMode::Pf, &pf, row);
    }
    let mut x = pf.clone();
    let s = &oracle["solves"]["pf"]["solution"];
    x.insert(
        "station/ac_dc_converter_station/1/iconv".into(),
        (s["convdc"]["1"]["pdc"].as_f64().unwrap() / s["busdc"]["1"]["vm"].as_f64().unwrap()).abs(),
    );
    check(&net, EquationMode::Pf, &x, "/current");
}

#[test]
fn malformed_points_and_unsupported_profiles_fail_closed() {
    let (net, oracle, map) = fixture();
    let model = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Pf).unwrap();
    let x = reference::point(&net, &oracle["solves"]["pf"]["solution"], &map);
    for replacement in [f64::NAN, f64::INFINITY] {
        let mut bad = x.clone();
        bad.insert("vm[1]".into(), replacement);
        assert!(model.evaluate(&bad).is_err());
    }
    let mut missing = x.clone();
    missing.remove("vm[1]");
    assert!(model.evaluate(&missing).is_err());
    missing.insert("bogus".into(), 1.0);
    assert!(model.evaluate(&missing).is_err());
    for n in [
        mutated(&net, |e| {
            e.stations[0].technology = ConverterTechnology::Lcc
        }),
        mutated(&net, |e| {
            e.stations[0].dc_control = DcConverterControl::DcPowerDroop
        }),
        mutated(&net, |e| e.base_mva = 200.0),
    ] {
        assert!(ReferenceEquations::powermodels_acdc_0101(&n, EquationMode::Pf).is_err());
    }
}

#[test]
fn opf_dispatch_does_not_impose_pf_targets_and_loss_policy_is_explicit() {
    let (net, oracle, map) = fixture();
    let n = mutated(&net, |e| {
        e.stations[0].settings.pcc_withdrawal_mw += 50.0;
        e.stations[0].settings.pcc_withdrawal_mvar += 50.0;
        e.stations[0].losses.rectifier_quadratic_ohm *= 2.0;
    });
    let model = ReferenceEquations::powermodels_acdc_0101(&n, EquationMode::Opf).unwrap();
    let x = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
    let r = model.evaluate(&x).unwrap();
    assert!(r.max_violation < 1e-7);
    assert_ne!(
        r.station_policy[0].declared_rectifier_quadratic_ohm,
        r.station_policy[0].declared_inverter_quadratic_ohm
    );
    let mut fixed_angle = net.clone();
    fixed_angle.branches_mut()[0].angmin = 0.1;
    fixed_angle.branches_mut()[0].angmax = 0.1;
    let pf = ReferenceEquations::powermodels_acdc_0101(&fixed_angle, EquationMode::Pf).unwrap();
    let pf_point = reference::point(&net, &oracle["solves"]["pf"]["solution"], &map);
    assert!(pf.evaluate(&pf_point).unwrap().max_violation < 1e-7);
    check(&fixed_angle, EquationMode::Opf, &x, "angle_difference");
    let bounded = mutated(&net, |e| e.branches[0].ratings_mw[0] = 0.0);
    check(&bounded, EquationMode::Opf, &x, "dc_branch/");
}
