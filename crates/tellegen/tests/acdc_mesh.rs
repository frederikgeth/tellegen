#![cfg(feature = "acopf")]
#[path = "support/acdc_mesh.rs"]
mod mesh;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use mesh::*;
use optimized::{cost_close, start};
use powerio::{AcDcOpfInstance, AcDcOpfProfile};
use tellegen::ac_dc::{EquationMode, PreparedAcDcOpf, ReferenceEquations};
#[test]
fn mesh_physical_bounds_and_starts_are_base_invariant() {
    for case in cases() {
        let first = bases()[0];
        let reference = bounds(&case.prepared(first), first);
        let starts = case.starts(first);
        for b in bases() {
            let model = case.prepared(b);
            assert_eq!(model.manifest().angle_references.len(), 3);
            for (k, limits) in bounds(&model, b) {
                for (a, z) in limits.into_iter().zip(reference[&k]) {
                    match (a, z) {
                        (Some(a), Some(z)) => assert!((a - z).abs() < 1e-10),
                        (None, None) => {}
                        _ => panic!("bound mismatch {k}"),
                    }
                }
            }
            for ((_, x), (_, y)) in case.starts(b).into_iter().zip(&starts) {
                let y = first.physical(y);
                for (k, v) in b.physical(&x) {
                    assert!((v - y[&k]).abs() < 1e-10);
                }
            }
        }
    }
}
#[test]
fn mesh_dispatch_limits_base_conversion_and_station_identity() {
    for case in cases() {
        let mut points = vec![];
        for b in [bases()[0], bases()[3]] {
            let model = case.prepared(b);
            for (_, initial) in case.starts(b) {
                let r = model.solve(start(&model, initial), None).unwrap();
                let x = b.physical(&r.point);
                assert!(physical_residual(case, &x) < 1e-6);
                binding(case, &x);
                cost_close(r.validation.objective, case.cost(&x));
                points.push((r.validation.objective, x));
            }
        }
        for b in bases() {
            let model = case.prepared(b);
            for (objective, x) in &points {
                let r = model.evaluate(&b.per_unit(x)).unwrap();
                assert!(r.max_violation < 1e-7);
                cost_close(r.objective, *objective);
                cost_close(*objective, points[0].0);
            }
        }
        let b = bases()[3];
        let model = case.prepared(b);
        let x = b.per_unit(&points[0].1);
        let a = format!("station/{}/pdc", station(case.cheap));
        let c = format!("station/{}/pdc", station((case.cheap + 1) % 3));
        let mut swapped = x.clone();
        swapped.insert(a.clone(), x[&c]);
        swapped.insert(c.clone(), x[&a]);
        let bad = model.evaluate(&swapped).unwrap();
        for i in [case.cheap, (case.cheap + 1) % 3] {
            assert!(bad
                .equations
                .iter()
                .any(|e| e.name == format!("dc_bus/{}/balance", node(i)) && e.violation > 1e-3));
        }
        // The mesh must be connected by declared identities, not vector positions.
        let mut net = case.network(b);
        net.buses_mut().reverse();
        net.generators_mut().reverse();
        net.loads_mut().reverse();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.buses.reverse();
        owner.stations.reverse();
        owner.branches.rotate_left(1);
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let permuted = PreparedAcDcOpf::new(
            AcDcOpfInstance::from_network(net, AcDcOpfProfile::PhysicalCoordinatesAcpVscV1)
                .unwrap(),
        )
        .unwrap();
        let r = permuted.evaluate(&x).unwrap();
        assert!(r.max_violation < 1e-7);
        cost_close(r.objective, points[0].0);
        if case.cheap == 0 && case.poles == 2 {
            let intent = AcDcOpfInstance::from_network(
                case.network(b),
                AcDcOpfProfile::PhysicalCoordinatesAcpVscV1,
            )
            .unwrap();
            let equations = ReferenceEquations::from_instance(&intent, EquationMode::Opf).unwrap();
            let d = equations.check_derivatives(&x).unwrap();
            assert!(d.passed, "{}: {d:?}", case.group());
        }
    }
}
