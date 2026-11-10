#![cfg(feature = "acopf")]
#[path = "support/acdc_losses.rs"]
mod losses;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use losses::*;
use physical::*;
use powerio::{AcDcOpfInstance, AcDcOpfProfile};
use tellegen::ac_dc::{EquationMode, PreparedAcDcOpf, ReferenceEquations};
fn coordinates(b: Bases) -> optimized::Coordinates {
    optimized::Coordinates {
        mva: b.mva,
        pcc_kv: b.pcc_kv,
        converter_kv: b.converter_kv,
        dc_from_kv: b.dc_kv,
        dc_to_kv: b.dc_kv,
    }
}
#[test]
fn asymmetric_loss_witnesses_preserve_physics_across_bases() {
    for case in cases() {
        let c = case.circuit();
        let initial = bases()[0];
        let physical = coordinates(initial).physical(&c.point(initial));
        for b in bases() {
            let x = coordinates(b).per_unit(&physical);
            let model = case.equations(b);
            let r = model.evaluate(&x).unwrap();
            let expected = case.expected_loss_residual_mw();
            for e in &r.equations {
                if e.name == format!("station/{STATION}/loss") {
                    assert!((e.value * b.mva - expected).abs() < 1e-10);
                } else {
                    assert!(e.violation < 1e-9, "{}: {}", e.name, e.violation);
                }
            }
            assert!((r.max_violation - expected.abs() / b.mva).abs() < 1e-9);
            let policy = &r.station_policy[0];
            assert_eq!(
                policy.declared_rectifier_quadratic_ohm,
                case.coefficients[0]
            );
            assert_eq!(policy.declared_inverter_quadratic_ohm, case.coefficients[1]);
            assert!(
                (policy.loss_coefficients_pu[2] * b.mva / b.current_ka().powi(2)
                    - case.coefficients[1])
                    .abs()
                    < 1e-12
            );
        }
        if case.coefficients == [0.3, 1.3] && case.poles == 2 {
            let b = bases()[8];
            let d = case.equations(b).check_derivatives(&c.point(b)).unwrap();
            assert!(d.passed, "{case:?}: {d:?}");
        }
    }
}
#[test]
fn valve_sign_is_distinct_from_pcc_dc_and_idle_boundaries() {
    let base = Case {
        operating: "forward",
        coefficients: [0.3, 1.3],
        selected: "rectifier",
        mask: 7,
        poles: 2,
    };
    for operating in [
        "forward",
        "reverse",
        "pcc_sign_trap",
        "near_positive",
        "near_negative",
        "zero_valve",
        "zero_current",
        "dc_idle",
    ] {
        let rec = Case { operating, ..base };
        let inv = Case {
            selected: "inverter",
            ..rec
        };
        let a = rec.circuit();
        let b = inv.circuit();
        if operating == "dc_idle" {
            assert!(a.dc_from_mw.abs() < 1e-12 && b.dc_from_mw.abs() < 1e-12);
            assert!(a.valve_power.re > 0. && b.valve_power.re > 0.);
            assert!(a.valve_current.norm() > 0. && b.valve_current.norm() > 0.);
        } else {
            assert!((b.valve_loss_mw - a.valve_loss_mw - a.valve_current.norm_sqr()).abs() < 1e-12);
        }
        match operating {
            "forward" => assert!(a.valve_power.re > 0. && a.dc_from_mw > 0.),
            "reverse" => assert!(a.valve_power.re < 0. && a.dc_from_mw < 0.),
            "pcc_sign_trap" => assert!(a.pcc_power.re > 0. && a.valve_power.re < 0.),
            "near_positive" => assert!(a.valve_power.re > 0. && a.dc_from_mw < 0.),
            "zero_valve" => {
                assert!(a.valve_power.re.abs() < 1e-12);
                assert!(rec.named_mode().is_none());
                // With reactive current, two legitimate branch extensions have
                // different zero-P losses: a hard sign switch is discontinuous.
                assert!((b.valve_loss_mw - a.valve_loss_mw - 0.0064).abs() < 1e-12);
                assert!((a.valve_loss_mw - 0.45792).abs() < 1e-12);
                assert!((b.valve_loss_mw - 0.46432).abs() < 1e-12);
            }
            "zero_current" => {
                assert_eq!(a.valve_current.norm(), 0.);
                assert_eq!(a.valve_loss_mw, LOSS[0]);
                assert_eq!(b.valve_loss_mw, LOSS[0]);
                assert!(a.dc_from_mw < 0.);
            }
            _ => {}
        }
    }
    // Limits from each side retain a finite 6.4 kW jump as valve P tends to zero.
    let left = Case {
        operating: "near_negative",
        selected: "inverter",
        ..base
    }
    .circuit();
    let right = Case {
        operating: "near_positive",
        selected: "rectifier",
        ..base
    }
    .circuit();
    assert!((left.valve_loss_mw - right.valve_loss_mw - 0.0064).abs() < 1e-12);
}
#[test]
fn source_profiles_keep_inverter_loss_and_retain_both_coefficients_in_ir() {
    let profiles = [
        AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
        AcDcOpfProfile::DeclaredCurrentAcpVscV1,
        AcDcOpfProfile::PhysicalCoordinatesAcpVscV1,
    ];
    for operating in ["forward", "reverse", "zero_valve", "dc_idle"] {
        let case = Case {
            operating,
            coefficients: [0.3, 1.3],
            selected: "inverter",
            mask: 7,
            poles: 2,
        };
        for b in bases() {
            let net = case.network(b);
            let point = case.circuit().point(b);
            let mut changed = net.clone();
            let mut owner = changed.ac_dc_equivalent().unwrap().clone();
            owner.stations[0].losses.rectifier_quadratic_ohm = 2.7;
            changed.set_ac_dc_equivalent(Some(owner)).unwrap();
            for profile in profiles {
                let a = AcDcOpfInstance::from_network(net.clone(), profile).unwrap();
                let z = AcDcOpfInstance::from_network(changed.clone(), profile).unwrap();
                let before = ReferenceEquations::from_instance(&a, EquationMode::Opf)
                    .unwrap()
                    .evaluate(&point)
                    .unwrap();
                let after = ReferenceEquations::from_instance(&z, EquationMode::Opf)
                    .unwrap()
                    .evaluate(&point)
                    .unwrap();
                assert!(before.max_violation < 1e-9 && after.max_violation < 1e-9);
                assert_eq!(
                    serde_json::to_value(&before.equations).unwrap(),
                    serde_json::to_value(&after.equations).unwrap()
                );
                assert_eq!(before.objective, after.objective);
                // Retained source provenance still changes preparation identity.
                assert_ne!(
                    PreparedAcDcOpf::new(a)
                        .unwrap()
                        .manifest()
                        .model_fingerprint,
                    PreparedAcDcOpf::new(z)
                        .unwrap()
                        .manifest()
                        .model_fingerprint
                );
            }
            let source = serde_json::to_value(&net).unwrap();
            let emitted = powerio::serialize(
                &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(net)),
                powerio::Destination::memory("loss.json").unwrap(),
            )
            .unwrap();
            let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
                panic!()
            };
            let restored = powerio::deserialize(
                powerio::Source::from_memory("loss.json", artifacts[0].bytes().to_vec()).unwrap(),
            )
            .unwrap();
            let powerio::PioValue::BalancedNetwork(restored) = restored.value() else {
                panic!()
            };
            assert_eq!(serde_json::to_value(restored).unwrap(), source);
        }
    }
}
