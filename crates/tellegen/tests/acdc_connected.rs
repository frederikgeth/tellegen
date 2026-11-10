#![cfg(feature = "acopf")]
#[path = "support/acdc_connected.rs"]
mod connected;
#[path = "support/acdc_mesh.rs"]
mod mesh;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use connected::*;
use mesh::{bases, generator, station};
use optimized::{cost_close, start};
use powerio::{AcDcAngleReferencePolicy, AcDcOpfInstance, AcDcOpfProfile, BusType};
use tellegen::ac_dc::{EquationMode, PreparedAcDcOpf, ReferenceEquations};
fn prepare(net: powerio::BalancedNetwork) -> PreparedAcDcOpf {
    PreparedAcDcOpf::new(
        AcDcOpfInstance::from_network(net, AcDcOpfProfile::PhysicalCoordinatesAcpVscV1)
            .unwrap()
            .with_angle_reference_policy(AcDcAngleReferencePolicy::LowestBusIdPerIsland),
    )
    .unwrap()
}
#[test]
fn outages_preserve_source_and_only_ac_edges_determine_angle_references() {
    for case in cases() {
        let mut reference_bounds = None;
        let reference_starts = case.starts(bases()[0]);
        for b in bases() {
            let net = case.network(b);
            let source = serde_json::to_value(&net).unwrap();
            let model = prepare(net.clone());
            let m = model.manifest();
            assert_eq!(m.variables.len(), case.variable_count());
            assert_eq!(
                m.angle_references
                    .iter()
                    .map(|r| r.bus.0)
                    .collect::<Vec<_>>(),
                case.expected_anchors()
            );
            for anchor in &m.angle_references {
                assert_eq!(anchor.declared, anchor.bus.0 == 11);
            }
            assert_eq!(
                net.buses().iter().map(|b| b.kind).collect::<Vec<_>>(),
                vec![BusType::Ref, BusType::Pv, BusType::Pv]
            );
            let strict = PreparedAcDcOpf::new(
                AcDcOpfInstance::from_network(
                    net.clone(),
                    AcDcOpfProfile::PhysicalCoordinatesAcpVscV1,
                )
                .unwrap(),
            );
            assert_eq!(strict.is_ok(), case.expected_anchors().len() == 1);
            let owner = net.ac_dc_equivalent().unwrap();
            assert_eq!(
                (
                    net.branches().len(),
                    owner.branches.len(),
                    owner.stations.len()
                ),
                (3, 3, 3)
            );
            for i in 0..3 {
                assert_eq!(net.branches()[i].in_service, case.ac_active(i));
                assert_eq!(owner.branches[i].in_service, case.dc_active(i));
                assert_eq!(owner.stations[i].in_service, case.station_active(i));
                for (prefix, active) in [
                    (format!("ac_branch/{}/", ac_branch(i)), case.ac_active(i)),
                    (format!("dc_branch/{}/", mesh::branch(i)), case.dc_active(i)),
                    (format!("station/{}/", station(i)), case.station_active(i)),
                ] {
                    assert_eq!(
                        m.variables.iter().any(|v| v.name.starts_with(&prefix)),
                        active
                    );
                }
            }
            let current = bounds(&model, b);
            let reference = reference_bounds.get_or_insert_with(|| current.clone());
            for (k, limits) in current {
                for (x, y) in limits.into_iter().zip(reference[&k]) {
                    match (x, y) {
                        (Some(x), Some(y)) => assert!((x - y).abs() < 1e-10),
                        (None, None) => {}
                        _ => panic!("{k}"),
                    }
                }
            }
            for ((_, x), (_, y)) in case.starts(b).into_iter().zip(&reference_starts) {
                let expected = physical(bases()[0], y);
                for (k, v) in physical(b, &x) {
                    assert!((v - expected[&k]).abs() < 1e-10);
                }
            }
            let emitted = powerio::serialize(
                &powerio::PioModule::new(powerio::PioValue::BalancedNetwork(net)),
                powerio::Destination::memory("outage.json").unwrap(),
            )
            .unwrap();
            let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
                panic!()
            };
            let restored = powerio::deserialize(
                powerio::Source::from_memory("outage.json", artifacts[0].bytes().to_vec()).unwrap(),
            )
            .unwrap();
            let powerio::PioValue::BalancedNetwork(restored) = restored.value() else {
                panic!()
            };
            assert_eq!(serde_json::to_value(restored).unwrap(), source);
            assert_eq!(
                prepare(restored.clone()).manifest().model_fingerprint,
                m.model_fingerprint
            );
        }
    }
}
#[test]
fn connected_outage_dispatch_crosses_bases_and_preserves_equipment_identity() {
    for case in cases() {
        let mut points = vec![];
        for b in [bases()[0], bases()[3]] {
            let model = case.prepared(b);
            for (_, initial) in case.starts(b) {
                let r = model.solve(start(&model, initial), None).unwrap();
                let x = physical(b, &r.point);
                assert!(physical_residual(case, &x) < 1e-6, "{}", case.group());
                cost_close(r.validation.objective, case.base().cost(&x));
                points.push((r.validation.objective, x));
            }
        }
        for b in bases() {
            let model = case.prepared(b);
            for (objective, x) in &points {
                let r = model.evaluate(&per_unit(b, x)).unwrap();
                assert!(r.max_violation < 1e-7);
                cost_close(r.objective, *objective);
                cost_close(*objective, points[0].0);
            }
        }
        let b = bases()[3];
        let x = per_unit(b, &points[0].1);
        let mut net = case.network(b);
        net.buses_mut().reverse();
        net.generators_mut().reverse();
        net.loads_mut().reverse();
        net.branches_mut().reverse();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.buses.reverse();
        owner.branches.reverse();
        owner.stations.reverse();
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let r = prepare(net).evaluate(&x).unwrap();
        assert!(r.max_violation < 1e-7);
        cost_close(r.objective, points[0].0);
        if case.cheap == 0 && case.poles == 2 {
            let eq = ReferenceEquations::from_instance(&case.intent(b), EquationMode::Opf).unwrap();
            let d = eq.check_derivatives(&x).unwrap();
            assert!(d.passed, "{}: {d:?}", case.group());
        }
    }
}
#[test]
fn outage_rejects_stale_starts_and_dc_connection_does_not_fix_ac_angles() {
    let b = bases()[3];
    let intact = Case {
        topology: 0,
        cheap: 0,
        poles: 2,
    };
    let old = intact.prepared(b);
    let old_point = intact.starts(b)[0].1.clone();
    for topology in 1..8 {
        let case = Case { topology, ..intact };
        let model = case.prepared(b);
        assert_ne!(
            model.manifest().model_fingerprint,
            old.manifest().model_fingerprint
        );
        assert!(model
            .initial_point(&start(&old, old_point.clone()))
            .is_err());
        assert!(model
            .initial_point(&start(&model, old_point.clone()))
            .is_err());
    }
    // Rotate an entire AC island, including the converter internal angles.
    // DC transfer and every electrical equation remain unchanged; its anchor moves.
    for topology in [0, 2] {
        let case = Case { topology, ..intact };
        let model = case.prepared(b);
        let r = model
            .solve(start(&model, case.starts(b)[0].1.clone()), None)
            .unwrap();
        let mut shifted = r.point.clone();
        for k in [
            "va[13]".to_string(),
            format!("station/{}/vafilt", station(2)),
            format!("station/{}/vaconv", station(2)),
        ] {
            *shifted.get_mut(&k).unwrap() += 0.01;
        }
        let check = model.evaluate(&shifted).unwrap();
        let violations: Vec<_> = check
            .equations
            .iter()
            .filter(|e| e.violation > 1e-7)
            .collect();
        if topology == 2 {
            assert_eq!(violations.len(), 1, "{violations:?}");
            assert_eq!(violations[0].name, "reference_angle[13]");
        } else {
            assert!(violations.iter().any(|e| e.name.starts_with("ac_branch/")));
        }
    }
}
#[test]
fn disconnected_load_has_no_hidden_supply_or_automatic_shedding() {
    let case = Case {
        topology: 7,
        cheap: 0,
        poles: 2,
    };
    let b = bases()[0];
    let mut net = case.network(b);
    net.generators_mut()[2].in_service = false;
    let model = prepare(net);
    let mut x = case.starts(b)[0].1.clone();
    x.remove(&format!("pg[{}]", generator(2)));
    x.remove(&format!("qg[{}]", generator(2)));
    // No branch, converter or generator reaches bus 13. Its active balance is
    // the nonzero constant -65 MW, independent of every remaining coordinate.
    for scale in [1.0, 1.01] {
        let y = x.iter().map(|(k, v)| (k.clone(), v * scale)).collect();
        let report = model.evaluate(&y).unwrap();
        let row = report
            .equations
            .iter()
            .find(|e| e.name == "p_balance[13]")
            .unwrap();
        assert!((row.violation - case.base().load(2) / b.mva).abs() < 1e-12);
    }
    assert!(model.solve(start(&model, x), None).is_err());
}
