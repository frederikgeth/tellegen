#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_shared.rs"]
mod fixture;
use powerio::{AcDcPfInstance, AcDcPfProfile};
use powerio_tx::acdc::AcConverterControl;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf, ReferenceEquations};
#[test]
fn shared_voltage_pf_allocates_injections_without_editing_source_controls() {
    for case in 0..fixture::CASES.len() {
        let i = fixture::instance(case);
        let net = i.network().clone();
        let original = serde_json::to_value(&net).unwrap();
        let model = PreparedAcDcPf::new(i).unwrap();
        assert_eq!(
            model.manifest().variables.len(),
            model.manifest().equalities.len()
        );
        let r = model
            .solve(AcDcPfStart::Declared, None)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture::CASES[case]));
        assert!(r.validation.max_violation < 1e-9);
        assert_eq!(
            serde_json::to_value(r.solution.network()).unwrap(),
            original
        );
        for bus in net.buses() {
            let mut increments = vec![];
            for g in net
                .generators()
                .iter()
                .filter(|g| g.in_service && g.bus == bus.id)
            {
                increments.push(
                    r.point[&format!("qg[{}]", g.uid.as_deref().unwrap())] - g.qg / net.base_mva(),
                );
            }
            for s in net.ac_dc_equivalent().unwrap().stations.iter().filter(|s| {
                s.in_service
                    && s.ac_bus == bus.id
                    && s.ac_control == AcConverterControl::VoltageAtPcc
            }) {
                increments.push(
                    -r.point[&format!("station/{}/qgrid", s.component)]
                        + s.settings.pcc_withdrawal_mvar / net.base_mva(),
                );
            }
            if let Some(first) = increments.first() {
                assert!(
                    increments.iter().all(|v| (v - first).abs() < 1e-9),
                    "{} bus {}: {increments:?}",
                    fixture::CASES[case],
                    bus.id.0
                );
                assert!((r.point[&format!("vm[{}]", bus.id.0)] - bus.vm).abs() < 1e-9);
            }
        }
        if case == 6 {
            assert!(!r
                .point
                .contains_key("station/ac_dc_converter_station/3/qgrid"));
        }
        if case == 7 {
            assert!(r
                .limit_violations
                .iter()
                .any(|e| e.name.ends_with("/iconv")));
        }
        let out = powerio::serialize(
            &powerio::PioModule::new(powerio::PioValue::from(r.solution.clone())),
            powerio::Destination::memory("shared.json").unwrap(),
        )
        .unwrap();
        let powerio::EmittedOutput::Memory { artifacts } = out.output() else {
            panic!()
        };
        let restored = powerio::deserialize(
            powerio::Source::from_memory("shared.json", artifacts[0].bytes().to_vec()).unwrap(),
        )
        .unwrap();
        let powerio::PioValue::AcDcPfSolution(s) = restored.value() else {
            panic!()
        };
        assert_eq!(s.instance().profile(), fixture::PROFILE);
        assert_eq!(s.point(), r.solution.point());
    }
}
#[test]
fn shared_voltage_pf_keeps_old_refusals_and_invalidates_profile_starts() {
    let old = AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1;
    for case in [0, 2, 3, 4, 5, 6, 7] {
        assert!(AcDcPfInstance::from_network(fixture::network(case), old).is_err());
    }
    let old = PreparedAcDcPf::new(AcDcPfInstance::from_network(fixture::network(1), old).unwrap())
        .unwrap();
    let new = PreparedAcDcPf::new(fixture::instance(1)).unwrap();
    assert!(new
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: old.manifest().model_fingerprint.clone(),
            values: old.initial_point(&AcDcPfStart::Declared).unwrap()
        })
        .is_err());
    let a = old.solve(AcDcPfStart::Declared, None).unwrap();
    let b = new.solve(AcDcPfStart::Declared, None).unwrap();
    for (k, v) in a.point {
        assert!((v - b.point[&k]).abs() < 1e-10);
    }
}
#[test]
fn shared_voltage_derivatives_and_allocation_are_independent_of_row_order() {
    for case in [2, 4, 5] {
        let instance = fixture::instance(case);
        let model = PreparedAcDcPf::new(instance.clone()).unwrap();
        let r = model.solve(AcDcPfStart::Declared, None).unwrap();
        assert!(
            ReferenceEquations::from_pf_instance(&instance)
                .unwrap()
                .check_derivatives(&r.point)
                .unwrap()
                .passed
        );
        let mut net = instance.network().clone();
        net.generators_mut().reverse();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.stations.reverse();
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let other =
            PreparedAcDcPf::new(AcDcPfInstance::from_network(net, fixture::PROFILE).unwrap())
                .unwrap()
                .solve(AcDcPfStart::Declared, None)
                .unwrap();
        for (k, v) in r.point {
            assert!((v - other.point[&k]).abs() < 1e-9);
        }
    }
}
