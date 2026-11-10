#![cfg(feature = "acopf")]
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use optimized::*;
use physical::*;
#[test]
fn all_fixture_bases_preserve_physical_bounds_and_starts() {
    for mask in 0..8 {
        for direction in [-1., 1.] {
            for poles in [1, 2] {
                let c = Circuit::with_elements(direction, poles, Elements::from_mask(mask));
                let first = coordinates()[0];
                let reference = physical_bounds(&prepared(&c, first), first);
                let original = starts(&c, first);
                for b in coordinates() {
                    let model = prepared(&c, b);
                    for (key, limits) in physical_bounds(&model, b) {
                        for (a, z) in limits.into_iter().zip(reference[&key]) {
                            match (a, z) {
                                (Some(a), Some(z)) => {
                                    assert!((a - z).abs() < 1e-10, "{key}: {a} != {z}")
                                }
                                (None, None) => {}
                                _ => panic!("different bound {key}"),
                            }
                        }
                    }
                    for ((_, x), (_, y)) in starts(&c, b).into_iter().zip(&original) {
                        for (k, v) in b.physical(&x) {
                            assert!((v - first.physical(y)[&k]).abs() < 1e-10);
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn optimized_physical_solutions_cross_bases_with_equal_cost() {
    for mask in 0..8 {
        for direction in [-1., 1.] {
            let c = Circuit::with_elements(direction, 2, Elements::from_mask(mask));
            let bases = [coordinates()[0], coordinates()[3]];
            let mut solutions = Vec::new();
            for b in bases {
                let model = prepared(&c, b);
                for (name, x) in starts(&c, b) {
                    let initial_cost = cost(&b.physical(&x));
                    let r = model.solve(start(&model, x), None).unwrap();
                    let physical = b.physical(&r.point);
                    assert!(physical_residual(&c, &physical) < 1e-6);
                    if name == "analytical" {
                        assert!(r.validation.objective < initial_cost - 0.01);
                    }
                    assert!(r.iterations > 0);
                    solutions.push((r.validation.objective, physical));
                }
            }
            for b in bases {
                let model = prepared(&c, b);
                for (objective, physical) in &solutions {
                    let r = model.evaluate(&b.per_unit(physical)).unwrap();
                    assert!(r.max_violation < 1e-7);
                    cost_close(r.objective, *objective);
                    cost_close(*objective, solutions[0].0);
                }
            }
            // Retaining pu voltage while changing its base is not physical conversion.
            let model = prepared(&c, bases[1]);
            let wrong = bases[0].per_unit(&solutions[0].1);
            assert!(model.evaluate(&wrong).unwrap().max_violation > 1e-3);
        }
    }
}
