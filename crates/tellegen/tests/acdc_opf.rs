#![cfg(feature = "acopf")]
#[path = "support/acdc_reference.rs"]
mod reference;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};
use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfStart, PreparedAcDcOpf};
fn fixture() -> (
    powerio::BalancedNetwork,
    serde_json::Value,
    serde_json::Value,
) {
    reference::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc"))
}
fn instance(net: &powerio::BalancedNetwork) -> powerio::AcDcOpfInstance {
    powerio::AcDcOpfInstance::from_network(net.clone(), AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1)
        .unwrap()
}

#[test]
fn native_case5_opf_converges_from_reference_declared_and_flat_starts() {
    let (net, oracle, map) = fixture();
    let source = serde_json::to_value(&net).unwrap();
    let prepared = PreparedAcDcOpf::new(instance(&net)).unwrap();
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
        let module = powerio::PioModule::new(powerio::PioValue::from(result.solution.clone()));
        let emitted = powerio::serialize(
            &module,
            powerio::Destination::memory("solution.json").unwrap(),
        )
        .unwrap();
        let powerio::EmittedOutput::Memory { mut artifacts } = emitted.into_output() else {
            panic!()
        };
        let restored = powerio::deserialize(
            powerio::Source::from_memory("solution.json", artifacts.pop().unwrap().into_bytes())
                .unwrap(),
        )
        .unwrap();
        let powerio::PioValue::AcDcOpfSolution(solution) = restored.value() else {
            panic!()
        };
        assert_eq!(solution.point(), result.solution.point());
        let supplied = instance(&net)
            .with_initial_point(solution.point().clone())
            .unwrap();
        let warm = PreparedAcDcOpf::new(supplied).unwrap();
        let restored_point = warm.initial_point(&AcDcOpfStart::InstancePoint).unwrap();
        assert!(warm.evaluate(&restored_point).unwrap().max_violation < 1e-7);

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
    let prepared = PreparedAcDcOpf::new(instance(&net)).unwrap();
    let fingerprint = prepared.manifest().model_fingerprint.clone();
    let repeated = PreparedAcDcOpf::new(instance(&net)).unwrap();
    assert_eq!(fingerprint, repeated.manifest().model_fingerprint);
    let alternate_policy = PreparedAcDcOpf::new(
        instance(&net)
            .with_angle_reference_policy(powerio::AcDcAngleReferencePolicy::LowestBusIdPerIsland),
    )
    .unwrap();
    assert_ne!(fingerprint, alternate_policy.manifest().model_fingerprint);

    let mut modified = net.clone();
    let mut owner = modified.ac_dc_equivalent().unwrap().clone();
    owner.stations[0].losses.constant_mw += 0.1;
    modified.set_ac_dc_equivalent(Some(owner)).unwrap();
    assert_ne!(
        fingerprint,
        PreparedAcDcOpf::new(instance(&modified))
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
    assert!(prepared
        .solve(AcDcOpfStart::Flat, Some(Arc::new(AtomicBool::new(true))))
        .unwrap_err()
        .contains("cancelled"));
}

#[test]
fn numerical_reference_never_creates_pf_dispatch_controls() {
    use tellegen::ac_dc::{EquationMode, ReferenceEquations};
    let (mut net, _, _) = fixture();
    for branch in net.branches_mut() {
        if branch.from.0 == 2 || branch.to.0 == 2 {
            branch.in_service = false;
        }
    }
    let selected = instance(&net)
        .with_angle_reference_policy(powerio::AcDcAngleReferencePolicy::LowestBusIdPerIsland);
    let error = ReferenceEquations::from_instance(&selected, EquationMode::Pf)
        .err()
        .unwrap();
    assert!(error.contains("do not qualify PF"), "{error}");
}
