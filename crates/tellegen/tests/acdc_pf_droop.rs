#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_droop.rs"]
mod fixture;
use powerio::{AcDcPfInstance, AcDcPfProfile};
use powerio_tx::acdc::DcConverterControl;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf, ReferenceEquations};
#[test]
fn case67_native_droop_solves_source_dispatch_controls_and_reports_limits() {
    let mut baseline = None;
    for case in 0..fixture::CASES.len() {
        let i = fixture::instance(case);
        let n = i.network().clone();
        let source = serde_json::to_value(&n).unwrap();
        let model = PreparedAcDcPf::new(i).unwrap();
        assert_eq!(
            model.manifest().variables.len(),
            model.manifest().equalities.len()
        );
        let r = model
            .solve(AcDcPfStart::Declared, None)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture::CASES[case]));
        assert!(r.validation.max_violation < 1e-9);
        assert_eq!(serde_json::to_value(r.solution.network()).unwrap(), source);
        let o = n.ac_dc_equivalent().unwrap();
        let mut droop_rows = 0;
        for s in o
            .stations
            .iter()
            .filter(|s| s.in_service && s.dc_control == DcConverterControl::DcPowerDroop)
        {
            droop_rows += 1;
            let p = r.point[&format!("station/{}/pdc", s.component)];
            let v = r.point[&format!("dc_bus/{}/vm", s.dc_bus)];
            let c = &s.settings;
            assert!(
                (p - c.dc_withdrawal_target_mw / n.base_mva()
                    + (v - c.dc_voltage_target_pu) / c.droop)
                    .abs()
                    < 1e-9
            );
            for (key, delta) in [
                (format!("station/{}/pdc", s.component), 0.01),
                (format!("dc_bus/{}/vm", s.dc_bus), 0.001),
            ] {
                let mut bad = r.point.clone();
                *bad.get_mut(&key).unwrap() += delta;
                assert!(model
                    .evaluate(&bad)
                    .unwrap()
                    .equations
                    .iter()
                    .any(
                        |e| e.name == format!("station/{}/dc_droop_setpoint", s.component)
                            && e.violation > 0.009
                    ));
            }
        }
        assert_eq!(
            model
                .manifest()
                .controls
                .iter()
                .filter(|s| s.ends_with("/dc_droop_setpoint"))
                .count(),
            droop_rows
        );
        let g = n.generators().iter().find(|g| g.bus.0 == 67).unwrap();
        let pg = r.point[&format!("pg[{}]", g.uid.as_deref().unwrap())] * n.base_mva();
        assert!((pg - if case == 6 { 810. } else { 800. }).abs() < 1e-7);
        assert!(!r.limit_violations.is_empty());
        if case == 0 {
            baseline = Some(r.point.clone());
        }
        if case == 7 {
            for (k, v) in baseline.as_ref().unwrap() {
                assert!((r.point[k] - v).abs() < 1e-9);
            }
        }
    }
}
#[test]
fn droop_profile_retains_old_refusals_and_rejects_invalid_or_stale_controls() {
    assert!(AcDcPfInstance::from_network(fixture::source(), fixture::PROFILE).is_err());
    for (name, n, expected) in fixture::refusals() {
        let error = AcDcPfInstance::from_network(n, fixture::PROFILE)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{name}: {error}");
    }
    let a = PreparedAcDcPf::new(fixture::instance(0)).unwrap();
    for case in [1, 2, 3, 4, 5, 6] {
        let b = PreparedAcDcPf::new(fixture::instance(case)).unwrap();
        assert!(b
            .initial_point(&AcDcPfStart::Point {
                model_fingerprint: a.manifest().model_fingerprint.clone(),
                values: a.initial_point(&AcDcPfStart::Declared).unwrap()
            })
            .is_err());
    }
    for old in [
        AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1,
        AcDcPfProfile::PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1,
    ] {
        assert!(AcDcPfInstance::from_network(fixture::network(0), old).is_err());
    }
    // Cutting bus 9 away leaves only its fixed-P station; remote droop cannot anchor it.
    let mut n = fixture::network(0);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.branches[10].in_service = false;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    assert!(AcDcPfInstance::from_network(n, fixture::PROFILE)
        .unwrap_err()
        .to_string()
        .contains("PF DC island"));
}
#[test]
fn droop_native_derivatives_cover_fixed_voltage_and_droop_only_grids() {
    for case in [0, 3, 4] {
        let i = fixture::instance(case);
        let model = PreparedAcDcPf::new(i.clone()).unwrap();
        let r = model.solve(AcDcPfStart::Declared, None).unwrap();
        let equations = ReferenceEquations::from_pf_instance(&i).unwrap();
        let mut perturbed = r.point.clone();
        for (k, v) in &mut perturbed {
            if k.ends_with("/pdc") {
                *v += 0.013;
            } else if k.ends_with("/vm") {
                *v *= 1.007;
            }
        }
        for x in [&r.point, &perturbed] {
            for c in equations.check_derivatives(x).unwrap().checks {
                assert!(c.max_scaled_error < 1e-6, "{c:?}");
            }
        }
    }
}

#[test]
fn droop_profile_changes_fingerprint_without_changing_old_case5_equations() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc/case5_3_he.m");
    let powerio::PioValue::BalancedNetwork(n) = powerio::parse(path).unwrap().into_value() else {
        panic!()
    };
    let old = PreparedAcDcPf::new(
        AcDcPfInstance::from_network(
            n.clone(),
            AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1,
        )
        .unwrap(),
    )
    .unwrap();
    let new =
        PreparedAcDcPf::new(AcDcPfInstance::from_network(n, fixture::PROFILE).unwrap()).unwrap();
    assert!(new
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: old.manifest().model_fingerprint.clone(),
            values: old.initial_point(&AcDcPfStart::Declared).unwrap()
        })
        .is_err());
    let a = old.solve(AcDcPfStart::Declared, None).unwrap();
    let b = new.solve(AcDcPfStart::Declared, None).unwrap();
    for (k, v) in &a.point {
        assert!((v - b.point[k]).abs() < 1e-9);
    }
    let out = powerio::serialize(
        &powerio::PioModule::new(powerio::PioValue::from(b.solution)),
        powerio::Destination::memory("droop.json").unwrap(),
    )
    .unwrap();
    let powerio::EmittedOutput::Memory { artifacts } = out.output() else {
        panic!()
    };
    let restored = powerio::deserialize(
        powerio::Source::from_memory("droop.json", artifacts[0].bytes().to_vec()).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        restored.into_value(),
        powerio::PioValue::AcDcPfSolution(_)
    ));
}
