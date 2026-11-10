#![cfg(feature = "acopf")]
#[path = "support/acdc_fixed_mode.rs"]
mod fixed;
#[path = "support/acdc_losses.rs"]
mod losses;
#[path = "support/acdc_mesh.rs"]
mod mesh;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use powerio::{AcDcOpfInstance, AcDcOpfProfile, AcDcStationLossMode, AcDcValveLossMode, PioValue};
use tellegen::ac_dc::{AcDcOpfStart, EquationMode, PreparedAcDcOpf, ReferenceEquations};
const PROFILE: AcDcOpfProfile = AcDcOpfProfile::FixedModePhysicalAcpVscV1;
fn intent(case: losses::Case, b: physical::Bases) -> AcDcOpfInstance {
    AcDcOpfInstance::from_network_with_loss_modes(
        case.network(b),
        PROFILE,
        vec![AcDcStationLossMode {
            component: physical::STATION.into(),
            mode: if case.selected == "rectifier" {
                AcDcValveLossMode::Rectifier
            } else {
                AcDcValveLossMode::Inverter
            },
        }],
    )
    .unwrap()
}
#[test]
fn declared_branches_enforce_valve_sign_at_all_loss_witnesses_and_bases() {
    let mut checks = 0;
    let mut derivatives = 0;
    for case in losses::cases() {
        for (base_index, b) in physical::bases().into_iter().enumerate() {
            let instance = intent(case, b);
            let model = ReferenceEquations::from_instance(&instance, EquationMode::Opf).unwrap();
            let x = case.circuit().point(b);
            let r = model.evaluate(&x).unwrap();
            assert!(
                r.equations.iter().all(|e| e.violation < 1e-9),
                "{case:?}: {:?}",
                r.equations
            );
            let p = case.circuit().valve_power.re;
            let wrong = if case.selected == "rectifier" {
                (-p).max(0.)
            } else {
                p.max(0.)
            };
            let bound = r
                .bounds
                .iter()
                .find(|v| v.name == format!("station/{}/pconv", physical::STATION))
                .unwrap();
            assert!((bound.violation * b.mva - wrong).abs() < 1e-12);
            if wrong > 0. {
                assert!(bound.violation > 0.);
            } else {
                assert!(r.max_violation < 1e-9);
            }
            let policy = &r.station_policy[0];
            assert_eq!(policy.loss_mode, Some(instance.loss_modes()[0].mode));
            assert!(
                (policy.loss_coefficients_pu[2] * b.mva / b.current_ka().powi(2)
                    - case.quadratic())
                .abs()
                    < 1e-12
            );
            if case.coefficients == [0.3, 1.3] && case.poles == 2 && base_index == 8 {
                assert!(model.check_derivatives(&x).unwrap().passed);
                derivatives += 1;
            }
            checks += 1;
        }
    }
    assert_eq!(checks, 1728);
    assert_eq!(derivatives, 32);
}
#[test]
fn modes_are_identity_keyed_persisted_and_part_of_start_identity_even_with_equal_losses() {
    let case = mesh::cases()[0];
    let b = mesh::bases()[0];
    let mut net = fixed::network(case, b);
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    for s in &mut owner.stations {
        s.losses.rectifier_quadratic_ohm = 1.3;
        s.losses.inverter_quadratic_ohm = 1.3;
    }
    owner.stations.reverse();
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    let modes = fixed::modes(case);
    let a =
        AcDcOpfInstance::from_network_with_loss_modes(net.clone(), PROFILE, modes.clone()).unwrap();
    let prepared = PreparedAcDcOpf::new(a.clone()).unwrap();
    for policy in &prepared.manifest().station_policy {
        assert_eq!(
            policy.loss_mode,
            Some(
                modes
                    .iter()
                    .find(|m| m.component == policy.component)
                    .unwrap()
                    .mode
            )
        );
    }
    assert!(ReferenceEquations::from_instance(&a, EquationMode::Pf).is_err());
    let mut reverse = modes.clone();
    reverse.reverse();
    let same = PreparedAcDcOpf::new(
        AcDcOpfInstance::from_network_with_loss_modes(net.clone(), PROFILE, reverse).unwrap(),
    )
    .unwrap();
    assert_eq!(
        prepared.manifest().model_fingerprint,
        same.manifest().model_fingerprint
    );
    let mut changed = modes;
    changed[0].mode = match changed[0].mode {
        AcDcValveLossMode::Rectifier => AcDcValveLossMode::Inverter,
        _ => AcDcValveLossMode::Rectifier,
    };
    let other = PreparedAcDcOpf::new(
        AcDcOpfInstance::from_network_with_loss_modes(net, PROFILE, changed).unwrap(),
    )
    .unwrap();
    assert_ne!(
        prepared.manifest().model_fingerprint,
        other.manifest().model_fingerprint
    );
    let x = case.starts(b)[0].1.clone();
    assert!(other
        .initial_point(&AcDcOpfStart::Point {
            model_fingerprint: prepared.manifest().model_fingerprint.clone(),
            values: x.clone()
        })
        .is_err());
    let typed = prepared.operating_point(&x).unwrap();
    let with_point = a.with_initial_point(typed).unwrap();
    let emitted = powerio::serialize(
        &powerio::PioModule::new(PioValue::from(with_point)),
        powerio::Destination::memory("fixed.json").unwrap(),
    )
    .unwrap();
    let powerio::EmittedOutput::Memory { artifacts } = emitted.output() else {
        panic!()
    };
    let restored = powerio::deserialize(
        powerio::Source::from_memory("fixed.json", artifacts[0].bytes().to_vec()).unwrap(),
    )
    .unwrap();
    let PioValue::AcDcOpfInstance(restored) = restored.value() else {
        panic!()
    };
    let rebuilt = PreparedAcDcOpf::new(restored.clone()).unwrap();
    assert_eq!(
        prepared.manifest().model_fingerprint,
        rebuilt.manifest().model_fingerprint
    );
    assert_eq!(
        serde_json::to_value(&prepared.manifest().station_policy).unwrap(),
        serde_json::to_value(&rebuilt.manifest().station_policy).unwrap()
    );
    assert_eq!(
        rebuilt.initial_point(&AcDcOpfStart::InstancePoint).unwrap(),
        x
    );
}
#[test]
fn fixed_mode_opf_preserves_physical_dispatch_and_active_limits_across_bases() {
    for case in mesh::cases().into_iter().filter(|c| c.cheap == 0) {
        let mut points = vec![];
        for b in mesh::bases() {
            let model = fixed::prepared(case, b);
            let r = model
                .solve(optimized::start(&model, case.starts(b)[0].1.clone()), None)
                .unwrap();
            let x = b.physical(&r.point);
            assert!(
                fixed::physical_residual(case, &x) < 1e-6,
                "{case:?} {b:?}: {} {:?}",
                fixed::physical_residual(case, &x),
                x
            );
            mesh::binding(case, &x);
            optimized::cost_close(case.cost(&x), r.validation.objective);
            points.push((r.validation.objective, x));
        }
        for b in mesh::bases() {
            let model = fixed::prepared(case, b);
            for (objective, x) in &points {
                let r = model.evaluate(&b.per_unit(x)).unwrap();
                assert!(r.max_violation < 1e-7);
                optimized::cost_close(*objective, r.objective);
            }
        }
    }
}
