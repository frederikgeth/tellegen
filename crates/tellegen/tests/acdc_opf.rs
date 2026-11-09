#![cfg(feature = "acopf")]
#[path = "support/acdc_reference.rs"]
mod reference;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfSpec, AcDcOpfStart, PreparedAcDcOpf};
fn fixture() -> (
    powerio::BalancedNetwork,
    serde_json::Value,
    serde_json::Value,
) {
    reference::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc"))
}
fn spec() -> AcDcOpfSpec {
    AcDcOpfSpec {
        profile: AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
    }
}

#[test]
fn native_case5_opf_converges_from_reference_declared_and_flat_starts() {
    let (net, oracle, map) = fixture();
    let source = serde_json::to_value(&net).unwrap();
    let prepared = PreparedAcDcOpf::new(&net, &spec()).unwrap();
    let reference = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
    let target = oracle["solves"]["opf"]["objective"].as_f64().unwrap();
    let warm = AcDcOpfStart::Point {
        model_fingerprint: prepared.manifest().model_fingerprint.clone(),
        values: reference.clone(),
    };
    for start in [warm, AcDcOpfStart::Declared, AcDcOpfStart::Flat] {
        let initial = prepared.initial_point(&start).unwrap();
        if !matches!(start, AcDcOpfStart::Point { .. }) {
            assert!(
                prepared.evaluate(&initial).unwrap().max_violation > 1e-3,
                "independent starts must not secretly use the oracle"
            );
            assert_ne!(initial, reference);
        }
        let result = prepared.solve(start, None).unwrap();
        assert_eq!(result.termination, "locally_solved");
        assert!(result.iterations > 0);
        assert!(result.validation.max_violation < 1e-7);
        assert!(result.solver_kkt_error < 1e-6);
        assert!((result.validation.objective - target).abs() < 1e-7 + 1e-8 * target.abs());
        assert_eq!(result.initial_point, initial);
        assert_eq!(result.point.len(), 95);
        assert_eq!(
            result.model_fingerprint,
            prepared.manifest().model_fingerprint
        );
    }
    assert_eq!(source, serde_json::to_value(&net).unwrap());
    assert!(powerio::AcOpfInstance::from_network(net).is_err());
}

#[test]
fn preparation_identity_includes_dc_data_and_start_roundtrips_fail_closed() {
    let (net, oracle, map) = fixture();
    let prepared = PreparedAcDcOpf::new(&net, &spec()).unwrap();
    let fingerprint = prepared.manifest().model_fingerprint.clone();
    let repeated = PreparedAcDcOpf::new(&net.clone(), &spec()).unwrap();
    assert_eq!(fingerprint, repeated.manifest().model_fingerprint);
    let mut modified = net.clone();
    let mut owner = modified.ac_dc_equivalent().unwrap().clone();
    owner.stations[0].losses.constant_mw += 0.1;
    modified.set_ac_dc_equivalent(Some(owner)).unwrap();
    assert_ne!(
        fingerprint,
        PreparedAcDcOpf::new(&modified, &spec())
            .unwrap()
            .manifest()
            .model_fingerprint
    );
    let point = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
    let start = AcDcOpfStart::Point {
        model_fingerprint: fingerprint.clone(),
        values: point.clone(),
    };
    let serialized = serde_json::to_string(&start).unwrap();
    let restored: AcDcOpfStart = serde_json::from_str(&serialized).unwrap();
    assert_eq!(prepared.initial_point(&restored).unwrap(), point);
    let wrong = AcDcOpfStart::Point {
        model_fingerprint: "stale".into(),
        values: point.clone(),
    };
    assert!(prepared.initial_point(&wrong).is_err());
    let mut missing = point.clone();
    missing.remove("vm[1]");
    assert!(prepared
        .initial_point(&AcDcOpfStart::Point {
            model_fingerprint: fingerprint.clone(),
            values: missing
        })
        .is_err());
    let mut bad = point;
    bad.insert("vm[1]".into(), f64::NAN);
    assert!(prepared
        .initial_point(&AcDcOpfStart::Point {
            model_fingerprint: fingerprint,
            values: bad
        })
        .is_err());
    let mut extra = serde_json::to_value(spec()).unwrap();
    extra["silent_projection"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<AcDcOpfSpec>(extra).is_err());
    let encoded = serde_json::to_string(&spec()).unwrap();
    assert_eq!(
        serde_json::from_str::<AcDcOpfSpec>(&encoded)
            .unwrap()
            .profile,
        spec().profile
    );
    for invalid in [
        "{}",
        r#"{"profile":"unknown"}"#,
        r#"{"profile":"power_models_ac_dc0101_acp_vsc_v1","silent_projection":true}"#,
    ] {
        assert!(serde_json::from_str::<AcDcOpfSpec>(invalid).is_err());
    }
    assert!(prepared
        .solve(AcDcOpfStart::Flat, Some(Arc::new(AtomicBool::new(true))))
        .unwrap_err()
        .contains("cancelled"));
}
