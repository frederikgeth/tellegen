#![cfg(feature = "acopf")]
#[path = "support/acdc_pf.rs"]
mod fixture;
#[path = "support/acdc_reference.rs"]
mod reference;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};
#[test]
fn case5_pf_controls_converge_from_independent_starts_and_roundtrip() {
    for case in 0..fixture::CASES.len() {
        let instance = fixture::instance(case);
        let source = serde_json::to_value(instance.network()).unwrap();
        let model = PreparedAcDcPf::new(instance).unwrap();
        assert_eq!(
            model.manifest().variables.len(),
            model.manifest().equalities.len()
        );
        let mut solutions = vec![];
        for start in [AcDcPfStart::Declared, AcDcPfStart::Flat] {
            let result = model
                .solve(start, None)
                .unwrap_or_else(|e| panic!("{}: {e}", fixture::CASES[case]));
            assert!(result.validation.max_violation < 1e-9);
            assert!(result.iterations > 0);
            let net = result.solution.network();
            assert_eq!(serde_json::to_value(net).unwrap(), source);
            for bus in net.buses() {
                let gens: Vec<_> = net
                    .generators()
                    .iter()
                    .filter(|g| g.in_service && g.bus == bus.id)
                    .collect();
                if gens.len() > 1 {
                    for g in &gens[1..] {
                        let first = gens[0];
                        for (field, a, b) in [("qg", g.qg, first.qg), ("pg", g.pg, first.pg)] {
                            if field == "pg" && bus.kind != powerio::BusType::Ref {
                                continue;
                            }
                            let value = |g: &powerio::Generator| {
                                result.point[&format!("{field}[{}]", g.uid.as_deref().unwrap())]
                                    * net.base_mva()
                            };
                            assert!((value(g) - a - value(first) + b).abs() < 1e-7);
                        }
                    }
                }
            }
            let out = powerio::serialize(
                &powerio::PioModule::new(powerio::PioValue::from(result.solution.clone())),
                powerio::Destination::memory("pf.json").unwrap(),
            )
            .unwrap();
            let powerio::EmittedOutput::Memory { artifacts } = out.output() else {
                panic!()
            };
            let restored = powerio::deserialize(
                powerio::Source::from_memory("pf.json", artifacts[0].bytes().to_vec()).unwrap(),
            )
            .unwrap();
            let powerio::PioValue::AcDcPfSolution(s) = restored.value() else {
                panic!()
            };
            let warm = PreparedAcDcPf::new(
                s.instance()
                    .clone()
                    .with_initial_point(s.point().clone())
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                model.manifest().model_fingerprint,
                warm.manifest().model_fingerprint
            );
            assert_eq!(
                warm.solve(AcDcPfStart::InstancePoint, None)
                    .unwrap()
                    .iterations,
                0
            );
            if case == 6 {
                assert!(result
                    .limit_violations
                    .iter()
                    .any(|e| e.name.ends_with("/iconv")));
            }
            if case == 4 || case == 5 {
                assert!((result.point["vm[2]"] - 1.01).abs() < 1e-10);
            }
            solutions.push(result.point);
        }
        for (k, v) in &solutions[0] {
            assert!((v - solutions[1][k]).abs() < 1e-7, "{case}: {k}");
        }
    }
}
#[test]
fn pf_preserves_reference_equations_but_controls_sharing_and_excludes_costs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc");
    let (net, oracle, map) = reference::load(&root);
    let mut x = reference::point(&net, &oracle["solves"]["pf"]["solution"], &map);
    let model = PreparedAcDcPf::new(fixture::instance(0)).unwrap();
    // The source reference leaves the two PV generators' reactive split free.
    let q = (x["qg[bus-1]"] + x["qg[bus-1-2]"]) / 2.;
    x.insert("qg[bus-1]".into(), q);
    x.insert("qg[bus-1-2]".into(), q);
    assert!(model.evaluate(&x).unwrap().max_violation < 1e-7);
    let solved = model.solve(AcDcPfStart::Declared, None).unwrap();
    for (k, v) in &solved.point {
        assert!((v - x[k]).abs() < 1e-7, "{k}");
    }
    let mut changed = net.clone();
    for g in changed.generators_mut() {
        g.cost = Some(powerio::GenCost::new(1, 0., 0., vec![0., 0., 1000., 100.]));
    }
    let instance = powerio::AcDcPfInstance::from_network(changed, fixture::PROFILE).unwrap();
    let other = PreparedAcDcPf::new(instance).unwrap();
    let r = other.solve(AcDcPfStart::Declared, None).unwrap();
    for (k, v) in solved.point {
        assert!((v - r.point[&k]).abs() < 1e-10);
    }
}
#[test]
fn pf_rejects_stale_nonphysical_and_cancelled_starts() {
    let model = PreparedAcDcPf::new(fixture::instance(0)).unwrap();
    let changed = PreparedAcDcPf::new(fixture::instance(1)).unwrap();
    let x = model.initial_point(&AcDcPfStart::Declared).unwrap();
    assert!(changed
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: model.manifest().model_fingerprint.clone(),
            values: x.clone()
        })
        .is_err());
    let mut bad = x;
    bad.insert("vm[1]".into(), 0.);
    assert!(model
        .solve(
            AcDcPfStart::Point {
                model_fingerprint: model.manifest().model_fingerprint.clone(),
                values: bad
            },
            None
        )
        .is_err());
    assert!(model
        .solve(
            AcDcPfStart::Declared,
            Some(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                true
            )))
        )
        .unwrap_err()
        .contains("cancelled"));
}

#[test]
fn pf_derivatives_identity_permutations_and_zero_width_ratings_are_checked() {
    for case in [0, 5, 7] {
        let instance = fixture::instance(case);
        let equations = tellegen::ac_dc::ReferenceEquations::from_pf_instance(&instance).unwrap();
        let model = PreparedAcDcPf::new(instance.clone()).unwrap();
        let r = model.solve(AcDcPfStart::Declared, None).unwrap();
        assert!(equations.check_derivatives(&r.point).unwrap().passed);
        let mut net = instance.network().clone();
        net.generators_mut().reverse();
        net.branches_mut().reverse();
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.stations.reverse();
        owner.buses.reverse();
        owner.branches.reverse();
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let other = PreparedAcDcPf::new(
            powerio::AcDcPfInstance::from_network(net, fixture::PROFILE).unwrap(),
        )
        .unwrap();
        let p = other.solve(AcDcPfStart::Declared, None).unwrap();
        for (key, value) in &r.point {
            assert!((value - p.point[key]).abs() < 1e-8, "{case}/{key}");
        }
    }
    let r = PreparedAcDcPf::new(fixture::instance(6))
        .unwrap()
        .solve(AcDcPfStart::Declared, None)
        .unwrap();
    assert!(r
        .limit_violations
        .iter()
        .any(|e| e.name.starts_with("angle_difference[") && e.violation > 1e-5));
}
