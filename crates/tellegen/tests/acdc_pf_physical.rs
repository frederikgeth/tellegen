#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_physical.rs"]
mod fixture;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use optimized::coordinates;
use physical::*;
use powerio::{AcDcCoordinatePolicy, AcDcOpfInstance, AcDcOpfProfile, AcDcPfInstance};
use tellegen::ac_dc::{AcDcPfStart, EquationMode, PreparedAcDcPf, ReferenceEquations};
#[test]
fn physical_pf_is_invariant_across_bases_bypasses_poles_controls_and_direction() {
    for mask in 0..8 {
        for direction in [-1., 1.] {
            for poles in [1, 2] {
                for droop in [false, true] {
                    let c = Circuit::with_elements(direction, poles, Elements::from_mask(mask));
                    for b in coordinates() {
                        let model = fixture::prepared(&c, b, droop);
                        for (_, start) in fixture::starts(&c, b, &model) {
                            let r = model.solve(start, None).unwrap();
                            assert_eq!(
                                r.validation.coordinate_policy,
                                AcDcCoordinatePolicy::PhysicalLocalBases
                            );
                            fixture::check(&c, b, droop, &r.point);
                        }
                    }
                }
            }
        }
    }
    // Exercise inherited shared-voltage allocation on physical local bases.
    // Equal injection increments require opposite signs for PCC withdrawal.
    for direction in [-1., 1.] {
        let c = Circuit::new(direction, 2);
        for b in coordinates() {
            let mut n = fixture::network(&c, b, true);
            n.generators_mut()[0].qg -= 5.;
            let mut o = n.ac_dc_equivalent().unwrap().clone();
            o.stations[0].ac_control = powerio_tx::acdc::AcConverterControl::VoltageAtPcc;
            o.stations[0].settings.pcc_withdrawal_mvar += 5.;
            n.set_ac_dc_equivalent(Some(o)).unwrap();
            let model =
                PreparedAcDcPf::new(AcDcPfInstance::from_network(n, fixture::PROFILE).unwrap())
                    .unwrap();
            let r = model.solve(AcDcPfStart::Declared, None).unwrap();
            fixture::check(&c, b, true, &r.point);
        }
    }
}
#[test]
fn physical_pf_reports_current_and_power_limits_without_saturation() {
    for direction in [-1., 1.] {
        for b in coordinates() {
            for droop in [false, true] {
                let c = Circuit::new(direction, 2);
                for limit in [0.3, c.valve_current.norm(), 0.2] {
                    let mut n = fixture::network(&c, b, droop);
                    let mut o = n.ac_dc_equivalent().unwrap().clone();
                    o.stations[0].valve_current_limit_pu = limit / b.bases().current_ka();
                    o.stations[0].valve_active_mw.min = -50.;
                    o.stations[0].valve_active_mw.max = 50.;
                    n.set_ac_dc_equivalent(Some(o)).unwrap();
                    let original = serde_json::to_value(&n).unwrap();
                    let m = PreparedAcDcPf::new(
                        AcDcPfInstance::from_network(n, fixture::PROFILE).unwrap(),
                    )
                    .unwrap();
                    let r = m.solve(AcDcPfStart::Declared, None).unwrap();
                    fixture::check(&c, b, droop, &r.point);
                    assert_eq!(
                        serde_json::to_value(r.solution.network()).unwrap(),
                        original
                    );
                    let current = r
                        .limit_violations
                        .iter()
                        .find(|e| e.name.ends_with("/iconv"));
                    if limit == 0.2 {
                        assert!(
                            (current.unwrap().violation * b.bases().current_ka()
                                - (c.valve_current.norm() - limit))
                                .abs()
                                < 1e-8
                        );
                    } else {
                        assert!(current.is_none());
                    }
                    let power = r
                        .limit_violations
                        .iter()
                        .find(|e| e.name.ends_with("/pconv"))
                        .unwrap();
                    assert!(
                        (power.violation * b.mva - (c.valve_power.re.abs() - 50.)).abs() < 1e-7
                    );
                    let output = powerio::serialize(
                        &powerio::PioModule::new(powerio::PioValue::from(r.solution)),
                        powerio::Destination::memory("physical.json").unwrap(),
                    )
                    .unwrap();
                    let powerio::EmittedOutput::Memory { artifacts } = output.output() else {
                        panic!()
                    };
                    assert!(powerio::deserialize(
                        powerio::Source::from_memory(
                            "physical.json",
                            artifacts[0].bytes().to_vec()
                        )
                        .unwrap()
                    )
                    .is_ok());
                }
            }
        }
    }
}
#[test]
fn physical_pf_rejects_stale_points_and_detects_wrong_base_conversions() {
    let c = Circuit::new(1., 2);
    let bases = coordinates();
    let b = bases[2];
    let correct = fixture::prepared(&c, b, true);
    let point = correct.solve(AcDcPfStart::Declared, None).unwrap().point;
    for which in 0..3 {
        let mut n = fixture::network(&c, b, true);
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        match which {
            0 => {
                o.stations[0].settings.droop =
                    fixture::DROOP_KV_PER_MW * bases[0].mva / bases[0].dc_from_kv
            }
            1 => {
                o.stations[0].settings.dc_voltage_target_pu =
                    fixture::DROOP_REFERENCE_KV / b.dc_to_kv
            }
            _ => o.buses[1].base_kv = b.dc_from_kv,
        }
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        let wrong = PreparedAcDcPf::new(AcDcPfInstance::from_network(n, fixture::PROFILE).unwrap())
            .unwrap();
        assert!(wrong.evaluate(&point).unwrap().max_violation > 1e-3);
        assert!(wrong
            .initial_point(&AcDcPfStart::Point {
                model_fingerprint: correct.manifest().model_fingerprint.clone(),
                values: point.clone()
            })
            .is_err());
    }
    let mut bad = point.clone();
    *bad.get_mut(&format!("station/{STATION}/iconv")).unwrap() *= 3_f64.sqrt();
    assert!(correct.evaluate(&bad).unwrap().max_violation > 0.1);
    let n = fixture::network(&c, b, true);
    let old =
        AcDcOpfInstance::from_network(n, AcDcOpfProfile::PhysicalCoordinatesAcpVscV1).unwrap();
    assert!(ReferenceEquations::from_instance(&old, EquationMode::Pf).is_err());
}
#[test]
fn physical_pf_derivatives_cover_droop_and_local_base_substitutions() {
    for (mask, direction, droop, index) in [(0, -1., true, 2), (7, 1., true, 3), (3, -1., false, 1)]
    {
        let c = Circuit::with_elements(direction, 2, Elements::from_mask(mask));
        let b = coordinates()[index];
        let i =
            AcDcPfInstance::from_network(fixture::network(&c, b, droop), fixture::PROFILE).unwrap();
        let model = PreparedAcDcPf::new(i.clone()).unwrap();
        let r = model.solve(AcDcPfStart::Declared, None).unwrap();
        let equations = ReferenceEquations::from_pf_instance(&i).unwrap();
        for point in [r.point, optimized::starts(&c, b).remove(1).1] {
            for check in equations.check_derivatives(&point).unwrap().checks {
                assert!(check.max_scaled_error < 1e-6, "{check:?}");
            }
        }
    }
}
