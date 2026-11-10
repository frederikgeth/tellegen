#![cfg(feature = "acopf")]
#[path = "support/acdc_physical.rs"]
mod physical;
use physical::*;
use powerio::{AcDcOpfInstance, AcDcOpfProfile, AcDcQuantity};
use tellegen::ac_dc::{EquationMode, PreparedAcDcOpf, ReferenceEquations};

fn intent(net: powerio::BalancedNetwork) -> AcDcOpfInstance {
    AcDcOpfInstance::from_network(net, AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1).unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}

#[test]
fn physical_circuit_is_invariant_across_power_and_voltage_bases() {
    for direction in [-1.0, 1.0] {
        for poles in [1, 2] {
            let c = Circuit::new(direction, poles);
            close(
                (c.pcc_power + c.transformer_to).re,
                3.0 * TRANSFORMER_OHM.re * c.transformer_current.norm_sqr(),
            );
            close(
                (c.reactor_from - c.valve_power).re,
                3.0 * REACTOR_OHM.re * c.valve_current.norm_sqr(),
            );
            close(
                c.dc_from_mw + c.dc_to_mw,
                f64::from(poles) * DC_OHM * c.dc_current_ka.powi(2),
            );
            for b in bases() {
                let net = c.network(b, 5.0);
                let original = serde_json::to_value(&net).unwrap();
                let x = c.point(b);
                for mode in [EquationMode::Pf, EquationMode::Opf] {
                    let r = ReferenceEquations::powermodels_acdc_0101(&net, mode)
                        .unwrap()
                        .evaluate(&x)
                        .unwrap();
                    assert!(r.max_violation < 1e-9, "{direction}, {poles}, {b:?}: {r:?}");
                    if mode == EquationMode::Opf {
                        close(r.objective, c.objective());
                    }
                    let [a, linear, quadratic] = r.station_policy[0].loss_coefficients_pu;
                    let i = x[&format!("station/{STATION}/iconv")];
                    close(
                        b.mva * (a + linear * i + quadratic * i * i),
                        c.valve_loss_mw,
                    );
                    close(
                        r.station_policy[0].effective_current_limit_pu * b.current_ka(),
                        5.0,
                    );
                }
                let prepared = PreparedAcDcOpf::new(intent(net.clone())).unwrap();
                let typed = prepared.operating_point(&x).unwrap();
                let quantity = |q| {
                    typed
                        .values()
                        .iter()
                        .find(|v| v.quantity == q)
                        .unwrap()
                        .value
                };
                close(quantity(AcDcQuantity::PccActivePower), c.pcc_power.re);
                close(quantity(AcDcQuantity::PccReactivePower), c.pcc_power.im);
                close(quantity(AcDcQuantity::ValveActivePower), c.valve_power.re);
                close(quantity(AcDcQuantity::StationDcActivePower), -c.dc_from_mw);
                close(
                    quantity(AcDcQuantity::ValveCurrentMagnitude) * b.current_ka(),
                    c.valve_current.norm(),
                );
                close(
                    quantity(AcDcQuantity::ValveVoltageMagnitude) * b.converter_kv,
                    200.0,
                );
                assert_eq!(serde_json::to_value(&net).unwrap(), original);
            }
        }
    }
}

#[test]
fn reference_current_limit_repair_is_not_voltage_base_invariant() {
    let c = Circuit::new(1.0, 2);
    for mva in [50.0, 100.0, 250.0] {
        let mut effective = Vec::new();
        for converter_kv in [200.0, 400.0] {
            let b = Bases {
                mva,
                pcc_kv: 230.0,
                converter_kv,
                dc_kv: 500.0,
            };
            let net = c.network(b, 0.05);
            let r = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf)
                .unwrap()
                .evaluate(&c.point(b))
                .unwrap();
            // Equations remain physically invariant; only the reference's
            // nominal-1-pu current-limit repair changes the feasible set.
            assert!(r.equations.iter().all(|e| e.violation < 1e-9));
            let current = r.station_policy[0].effective_current_limit_pu * b.current_ka();
            close(
                current,
                100.0_f64.hypot(100.0) / (3.0_f64.sqrt() * converter_kv),
            );
            effective.push(current);
            if converter_kv < 300.0 {
                assert!(r.max_violation < 1e-9);
            } else {
                assert!(r
                    .bounds
                    .iter()
                    .any(|v| v.name.ends_with("/iconv") && v.violation > 0.1));
            }
        }
        close(effective[0], 2.0 * effective[1]);
    }
}

#[test]
fn incorrect_unit_conversions_are_detected_and_generic_transforms_refuse() {
    let c = Circuit::new(1.0, 2);
    let b = Bases {
        mva: 250.0,
        pcc_kv: 400.0,
        converter_kv: 345.0,
        dc_kv: 800.0,
    };
    let net = c.network(b, 5.0);
    let x = c.point(b);
    let original = serde_json::to_value(&net).unwrap();
    assert!(net.to_normalized().is_err());
    assert!(powerio::AcPfInstance::from_network(net.clone()).is_err());
    assert!(powerio::AcOpfInstance::from_network(net.clone()).is_err());
    let mut mismatch = net.clone();
    *mismatch.base_mva_mut() = 100.0;
    assert!(
        AcDcOpfInstance::from_network(mismatch, AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1)
            .is_err()
    );
    for mutation in 0..6 {
        let mut changed = net.clone();
        let mut e = changed.ac_dc_equivalent().unwrap().clone();
        let s = &mut e.stations[0];
        let expected = match mutation {
            0 => {
                s.transformer_tap_ratio = TURNS_RATIO;
                "/transformer/"
            }
            1 => {
                s.transformer.resistance_pu *= (b.converter_kv / b.pcc_kv).powi(2);
                s.transformer.reactance_pu *= (b.converter_kv / b.pcc_kv).powi(2);
                "/transformer/"
            }
            2 => {
                s.filter_susceptance_pu *= (b.mva / 100.0).powi(2);
                "/filter_q"
            }
            3 => {
                s.ac_base_kv = b.pcc_kv;
                "/loss"
            }
            4 => {
                e.pole_count = 1;
                "dc_branch/"
            }
            _ => {
                e.branches[0].resistance_pu *= 100.0 / b.mva;
                "dc_branch/"
            }
        };
        changed.set_ac_dc_equivalent(Some(e)).unwrap();
        let r = ReferenceEquations::powermodels_acdc_0101(&changed, EquationMode::Opf)
            .unwrap()
            .evaluate(&x)
            .unwrap();
        assert!(
            r.equations
                .iter()
                .any(|v| v.name.contains(expected) && v.violation > 1e-6),
            "mutation {mutation}"
        );
    }
    let mut wrong_current = x;
    *wrong_current
        .get_mut(&format!("station/{STATION}/iconv"))
        .unwrap() *= 3.0_f64.sqrt();
    let r = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf)
        .unwrap()
        .evaluate(&wrong_current)
        .unwrap();
    assert!(r
        .equations
        .iter()
        .any(|v| v.name.ends_with("/current") && v.violation > 1e-3));
    assert_eq!(serde_json::to_value(&net).unwrap(), original);
}

#[test]
fn declared_current_policy_is_base_invariant_and_only_changes_the_current_bound() {
    use powerio::{
        AcDcCurrentLimitPolicy, Destination, EmittedOutput, PioModule, PioValue, Source,
    };
    use tellegen::ac_dc::AcDcOpfStart;
    for direction in [-1.0, 1.0] {
        for poles in [1, 2] {
            let c = Circuit::new(direction, poles);
            for limit in [0.05, c.valve_current.norm(), 0.3] {
                for b in bases() {
                    let net = c.network(b, limit);
                    let source = serde_json::to_value(&net).unwrap();
                    let instance = powerio::AcDcOpfInstance::from_network(
                        net.clone(),
                        AcDcOpfProfile::DeclaredCurrentAcpVscV1,
                    )
                    .unwrap();
                    assert!(
                        ReferenceEquations::from_instance(&instance, EquationMode::Pf).is_err()
                    );
                    let prepared = PreparedAcDcOpf::new(instance.clone()).unwrap();
                    let compat = PreparedAcDcOpf::new(intent(net)).unwrap();
                    let point = c.point(b);
                    let r = prepared.evaluate(&point).unwrap();
                    let old = compat.evaluate(&point).unwrap();
                    assert_eq!(r.current_limit_policy, AcDcCurrentLimitPolicy::Declared);
                    assert_eq!(
                        prepared.manifest().current_limit_policy,
                        AcDcCurrentLimitPolicy::Declared
                    );
                    assert_ne!(
                        prepared.manifest().model_fingerprint,
                        compat.manifest().model_fingerprint
                    );
                    close(
                        r.station_policy[0].effective_current_limit_pu * b.current_ka(),
                        limit,
                    );
                    close(r.objective, c.objective());
                    assert!(r.equations.iter().all(|e| e.violation < 1e-9));
                    for (new, old) in r
                        .equations
                        .iter()
                        .chain(&r.bounds)
                        .zip(old.equations.iter().chain(&old.bounds))
                    {
                        assert_eq!(new.name, old.name);
                        close(new.value, old.value);
                        close(new.lower, old.lower);
                        if !new.name.ends_with("/iconv") {
                            close(new.upper, old.upper);
                        }
                    }
                    let current = r
                        .bounds
                        .iter()
                        .find(|v| v.name.ends_with("/iconv"))
                        .unwrap();
                    close(
                        current.violation * b.current_ka(),
                        (c.valve_current.norm() - limit).max(0.0),
                    );
                    if limit >= c.valve_current.norm() {
                        assert!(r.max_violation < 1e-9);
                    } else {
                        assert!(r.max_violation > 0.1);
                    }
                    assert!(prepared
                        .initial_point(&AcDcOpfStart::Point {
                            model_fingerprint: compat.manifest().model_fingerprint.clone(),
                            values: point.clone()
                        })
                        .is_err());
                    // Typed physical result -> persisted intent -> exact supplied start.
                    // This preserves infeasible points too; serialization is not a solve.
                    let instance = instance
                        .with_initial_point(prepared.operating_point(&point).unwrap())
                        .unwrap();
                    let module = PioModule::new(PioValue::from(instance));
                    let emitted =
                        powerio::serialize(&module, Destination::memory("x.json").unwrap())
                            .unwrap();
                    let EmittedOutput::Memory { artifacts } = emitted.output() else {
                        panic!()
                    };
                    let restored = powerio::deserialize(
                        Source::from_memory("x.json", artifacts[0].bytes().to_vec()).unwrap(),
                    )
                    .unwrap();
                    let PioValue::AcDcOpfInstance(restored) = restored.value() else {
                        panic!()
                    };
                    let rebuilt = PreparedAcDcOpf::new(restored.clone()).unwrap();
                    assert_eq!(
                        rebuilt.manifest().model_fingerprint,
                        prepared.manifest().model_fingerprint
                    );
                    let x = rebuilt.initial_point(&AcDcOpfStart::InstancePoint).unwrap();
                    for (key, value) in &point {
                        close(x[key], *value);
                    }
                    close(rebuilt.evaluate(&x).unwrap().max_violation, r.max_violation);
                    assert_eq!(serde_json::to_value(restored.network()).unwrap(), source);
                }
            }
        }
    }
}
