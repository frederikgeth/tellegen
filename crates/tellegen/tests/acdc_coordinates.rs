#![cfg(feature = "acopf")]
#[path = "support/acdc_bypass.rs"]
mod bypass;
#[path = "support/acdc_physical.rs"]
mod physical;
use physical::*;
use powerio::{AcDcCoordinatePolicy, AcDcOpfInstance, AcDcOpfProfile, PioValue};
use tellegen::ac_dc::{AcDcOpfStart, EquationMode, PreparedAcDcOpf, ReferenceEquations};
const PROFILE: AcDcOpfProfile = AcDcOpfProfile::PhysicalCoordinatesAcpVscV1;
fn intent(net: powerio::BalancedNetwork) -> AcDcOpfInstance {
    AcDcOpfInstance::from_network(net, PROFILE).unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}

#[test]
fn physical_coordinates_accept_all_384_cases_and_persist_explicit_bases() {
    let cases = bypass::cases();
    assert_eq!(cases.len(), 384);
    for case in cases {
        let (net, point) = case.data();
        let source = serde_json::to_value(&net).unwrap();
        let instance = intent(net.clone());
        assert!(ReferenceEquations::from_instance(&instance, EquationMode::Pf).is_err());
        let prepared = PreparedAcDcOpf::new(instance.clone()).unwrap();
        let r = prepared.evaluate(&point).unwrap();
        assert!(
            r.max_violation < 1e-9,
            "{} {:?}: {r:?}",
            case.kind,
            case.bases
        );
        close(r.objective, case.circuit.objective());
        assert_eq!(
            r.coordinate_policy,
            AcDcCoordinatePolicy::PhysicalLocalBases
        );
        let m = prepared.manifest();
        assert_eq!(m.coordinate_policy, r.coordinate_policy);
        let p = &m.station_policy[0];
        let b = p.physical_bases.as_ref().unwrap();
        close(b.pcc_kv, case.bases.pcc_kv);
        close(b.filter_kv, case.bases.converter_kv);
        close(b.valve_kv, b.filter_kv);
        close(b.series_impedance_kv, case.bases.converter_kv);
        close(
            p.transformer_bypass_voltage_ratio,
            case.bases.pcc_kv / case.bases.converter_kv,
        );
        close(
            m.dc_branch_policy[0].resistance_base_kv.unwrap(),
            case.bases.dc_kv,
        );
        close(
            m.dc_branch_policy[0].to_voltage_scale,
            case.dc_to_base_kv / case.bases.dc_kv,
        );
        let old = PreparedAcDcOpf::new(
            AcDcOpfInstance::from_network(net, AcDcOpfProfile::DeclaredCurrentAcpVscV1).unwrap(),
        )
        .unwrap();
        let old_report = old.evaluate(&point).unwrap();
        let violations: std::collections::BTreeSet<_> = old_report
            .equations
            .iter()
            .chain(&old_report.bounds)
            .filter(|v| v.violation > 1e-9)
            .map(|v| v.name.clone())
            .collect();
        assert_eq!(violations, case.expected_violations().into_iter().collect());
        assert_ne!(m.model_fingerprint, old.manifest().model_fingerprint);
        assert!(prepared
            .initial_point(&AcDcOpfStart::Point {
                model_fingerprint: old.manifest().model_fingerprint.clone(),
                values: point.clone()
            })
            .is_err());
        // Persist the profile and typed values; no silent base or angle conversion.
        let instance = instance
            .with_initial_point(prepared.operating_point(&point).unwrap())
            .unwrap();
        let emitted = powerio::serialize(
            &powerio::PioModule::new(PioValue::from(instance)),
            powerio::Destination::memory("x.json").unwrap(),
        )
        .unwrap();
        let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
            panic!()
        };
        let restored = powerio::deserialize(
            powerio::Source::from_memory("x.json", artifacts[0].bytes().to_vec()).unwrap(),
        )
        .unwrap();
        let PioValue::AcDcOpfInstance(restored) = restored.value() else {
            panic!()
        };
        assert_eq!(serde_json::to_value(restored.network()).unwrap(), source);
        let restored = PreparedAcDcOpf::new(restored.clone()).unwrap();
        assert_eq!(m.model_fingerprint, restored.manifest().model_fingerprint);
        let x = restored
            .initial_point(&AcDcOpfStart::InstancePoint)
            .unwrap();
        for (k, v) in point {
            close(x[&k], v);
        }
        assert!(restored.evaluate(&x).unwrap().max_violation < 1e-9);
    }
}

#[test]
fn physical_dc_branch_orientation_and_wrong_reference_side_are_checked() {
    for case in bypass::cases()
        .into_iter()
        .filter(|c| c.kind == "unequal_dc_bases")
    {
        let (mut net, mut point) = case.data();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        let branch = &mut owner.branches[0];
        std::mem::swap(&mut branch.from, &mut branch.to);
        branch.resistance_pu *= (case.bases.dc_kv / case.dc_to_base_kv).powi(2);
        net.set_ac_dc_equivalent(Some(owner.clone())).unwrap();
        let (pf, pt) = (
            format!("dc_branch/{DC_BRANCH}/pf"),
            format!("dc_branch/{DC_BRANCH}/pt"),
        );
        let (a, b) = (point[&pf], point[&pt]);
        point.insert(pf, b);
        point.insert(pt, a);
        let prepared = PreparedAcDcOpf::new(intent(net.clone())).unwrap();
        assert!(prepared.evaluate(&point).unwrap().max_violation < 1e-9);
        close(
            prepared.manifest().dc_branch_policy[0]
                .resistance_base_kv
                .unwrap(),
            case.dc_to_base_kv,
        );
        // Reversing endpoints without rebasing impedance describes different hardware.
        owner.branches[0].resistance_pu = DC_OHM * case.bases.mva / case.bases.dc_kv.powi(2);
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let wrong = PreparedAcDcOpf::new(intent(net))
            .unwrap()
            .evaluate(&point)
            .unwrap();
        assert_eq!(
            wrong
                .equations
                .iter()
                .filter(|v| v.name.starts_with("dc_branch/") && v.violation > 1e-3)
                .count(),
            2
        );
    }
}

#[test]
fn weighted_equations_have_consistent_derivatives_and_detect_bad_base_metadata() {
    let mut count = 0;
    for case in bypass::cases().into_iter().filter(|c| {
        c.circuit.direction > 0.0
            && c.circuit.poles == 2
            && c.bases.mva == 100.0
            && (c.kind != "qualified" || c.bases.converter_kv == 200.0)
    }) {
        count += 1;
        let (mut net, point) = case.data();
        let model =
            ReferenceEquations::from_instance(&intent(net.clone()), EquationMode::Opf).unwrap();
        let derivatives = model.check_derivatives(&point).unwrap();
        assert!(derivatives.passed, "{}: {derivatives:?}", case.kind);
        let family = if case.kind == "unequal_pcc_bypass" {
            net.buses_mut()[0].base_kv *= 1.1;
            "/transformer/vm_bypass"
        } else {
            let mut owner = net.ac_dc_equivalent().unwrap().clone();
            owner.buses[1].base_kv *= 1.1;
            net.set_ac_dc_equivalent(Some(owner)).unwrap();
            "dc_branch/"
        };
        let wrong = PreparedAcDcOpf::new(intent(net))
            .unwrap()
            .evaluate(&point)
            .unwrap();
        assert!(wrong
            .equations
            .iter()
            .any(|v| v.name.contains(family) && v.violation > 1e-3));
    }
    assert_eq!(count, 16);
}
