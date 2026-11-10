#![cfg(feature = "acopf")]
#[path = "support/acdc_bypass.rs"]
mod bypass;
#[path = "support/acdc_physical.rs"]
mod physical;
use physical::*;
use powerio::{AcDcOpfInstance, AcDcOpfProfile};
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

fn report(
    net: &powerio::BalancedNetwork,
    x: &std::collections::BTreeMap<String, f64>,
    profile: AcDcOpfProfile,
) -> tellegen::ac_dc::EquationReport {
    let intent = AcDcOpfInstance::from_network(net.clone(), profile).unwrap();
    ReferenceEquations::from_instance(&intent, EquationMode::Opf)
        .unwrap()
        .evaluate(x)
        .unwrap()
}
fn close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-9 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}

#[test]
fn bypass_and_unequal_base_physical_qualification_is_explicit() {
    let mut counts = [0; 3];
    for case in bypass::cases() {
        let (net, x) = case.data();
        let source = serde_json::to_value(&net).unwrap();
        let c = &case.circuit;
        close(
            (c.pcc_power + c.transformer_to).re,
            if c.elements.transformer {
                3.0 * TRANSFORMER_OHM.re * c.transformer_current.norm_sqr()
            } else {
                0.0
            },
        );
        close(
            (c.reactor_from - c.valve_power).re,
            if c.elements.reactor {
                3.0 * REACTOR_OHM.re * c.valve_current.norm_sqr()
            } else {
                0.0
            },
        );
        close(
            c.dc_from_mw + c.dc_to_mw,
            f64::from(c.poles) * DC_OHM * c.dc_current_ka.powi(2),
        );
        for profile in [
            AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
            AcDcOpfProfile::DeclaredCurrentAcpVscV1,
        ] {
            let r = report(&net, &x, profile);
            let mut actual: Vec<_> = r
                .equations
                .iter()
                .chain(&r.bounds)
                .filter(|v| v.violation > 1e-9)
                .map(|v| v.name.clone())
                .collect();
            let mut expected = case.expected_violations();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "{} {:?}", case.kind, case.bases);
            close(r.objective, c.objective());
            assert_eq!(serde_json::to_value(&net).unwrap(), source);
        }
        // No generic transformation or legacy AC calculation may erase these distinctions.
        assert!(net.to_normalized().is_err());
        assert!(powerio::AcOpfInstance::from_network(net.clone()).is_err());
        let emitted = powerio::serialize(
            &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(net)),
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
        let powerio::PioValue::BalancedNetwork(restored) = restored.value() else {
            panic!()
        };
        assert_eq!(serde_json::to_value(restored).unwrap(), source);
        let i = match case.kind {
            "qualified" => 0,
            "unequal_pcc_bypass" => 1,
            _ => 2,
        };
        counts[i] += 1;
    }
    assert_eq!(counts, [288, 48, 48]);
}

#[test]
fn disabled_parameters_are_retained_ignored_and_take_effect_when_enabled() {
    for case in bypass::cases()
        .into_iter()
        .filter(|c| c.kind == "qualified")
    {
        let (net, x) = case.data();
        let profile = AcDcOpfProfile::DeclaredCurrentAcpVscV1;
        let old = report(&net, &x, profile);
        let mut changed = net.clone();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        let s = &mut owner.stations[0];
        if !s.transformer.enabled {
            s.transformer.resistance_pu *= 17.0;
            s.transformer.reactance_pu *= 13.0;
            s.transformer_tap_ratio = 0.73;
        }
        if !s.reactor.enabled {
            s.reactor.resistance_pu *= 19.0;
            s.reactor.reactance_pu *= 23.0;
        }
        if !s.filter_enabled {
            s.filter_susceptance_pu *= 29.0;
        }
        changed.set_ac_dc_equivalent(Some(owner.clone())).unwrap();
        let new = report(&changed, &x, profile);
        for (a, b) in old
            .equations
            .iter()
            .chain(&old.bounds)
            .zip(new.equations.iter().chain(&new.bounds))
        {
            assert_eq!(a.name, b.name);
            close(a.value, b.value);
            close(a.violation, b.violation);
        }
        let elements = case.circuit.elements;
        for (enabled, family) in [
            (elements.transformer, "/transformer/"),
            (elements.reactor, "/reactor/"),
            (elements.filter, "/filter_q"),
        ] {
            if enabled {
                continue;
            }
            let mut on = owner.clone();
            let s = &mut on.stations[0];
            match family {
                "/transformer/" => s.transformer.enabled = true,
                "/reactor/" => s.reactor.enabled = true,
                _ => s.filter_enabled = true,
            }
            changed.set_ac_dc_equivalent(Some(on)).unwrap();
            let r = report(&changed, &x, profile);
            assert!(
                r.equations
                    .iter()
                    .any(|v| v.name.contains(family) && v.violation > 1e-6),
                "{family}"
            );
        }
    }
}

#[test]
fn dc_base_metadata_and_single_resistance_rescaling_are_not_physical_rebasing() {
    for case in bypass::cases()
        .into_iter()
        .filter(|c| c.kind == "unequal_dc_bases")
    {
        let (mut net, x) = case.data();
        let profile = AcDcOpfProfile::DeclaredCurrentAcpVscV1;
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        // Using the other endpoint's Zbase cannot repair the missing voltage ratio.
        owner.branches[0].resistance_pu = DC_OHM * case.bases.mva / case.dc_to_base_kv.powi(2);
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let r = report(&net, &x, profile);
        assert!(r
            .equations
            .iter()
            .filter(|v| v.name.starts_with("dc_branch/"))
            .all(|v| v.violation > 1e-3));
        // Changing just metadata leaves source pu equations unchanged, but changes
        // reconstructed physical voltage. It is not a permitted physical projection.
        let mut net = case.circuit.network(case.bases, 5.0);
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.buses[1].base_kv = case.dc_to_base_kv;
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let x = case.circuit.point(case.bases);
        assert!(report(&net, &x, profile).max_violation < 1e-9);
        let reconstructed = x[&format!("dc_bus/{DC_TO}/vm")] * case.dc_to_base_kv;
        assert!((reconstructed - case.circuit.dc_to_kv).abs() > 100.0);
    }
}

#[test]
fn every_station_flag_combination_has_consistent_derivatives() {
    let cases = bypass::cases()
        .into_iter()
        .filter(|c| {
            c.kind == "qualified"
                && c.circuit.direction > 0.0
                && c.circuit.poles == 2
                && c.bases.mva == 100.0
                && c.bases.converter_kv == 200.0
        })
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let (net, point) = case.data();
        let intent =
            AcDcOpfInstance::from_network(net, AcDcOpfProfile::DeclaredCurrentAcpVscV1).unwrap();
        let model = ReferenceEquations::from_instance(&intent, EquationMode::Opf).unwrap();
        let derivatives = model.check_derivatives(&point).unwrap();
        assert!(
            derivatives.passed,
            "{:?}: {derivatives:?}",
            case.circuit.elements
        );
    }
}
