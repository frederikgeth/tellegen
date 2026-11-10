#![cfg(feature = "acopf")]
#[path = "support/acdc_connected.rs"]
mod connected;
#[path = "support/acdc_pf_fixed.rs"]
mod fixture;
#[path = "support/acdc_mesh.rs"]
mod mesh;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_pf_connected.rs"]
mod pf_connected;
#[path = "support/acdc_pf_physical.rs"]
mod pf_physical;
#[path = "support/acdc_physical.rs"]
mod physical;
use physical::*;
use powerio::{AcDcPfInstance, AcDcStationLossMode, AcDcValveLossMode};
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf, ReferenceEquations};
#[test]
fn fixed_mode_pf_solves_both_branches_and_zero_power_without_switching() {
    for case in fixture::cases() {
        for b in optimized::coordinates() {
            let m = case.prepared(b);
            let r = m
                .solve(AcDcPfStart::Declared, None)
                .unwrap_or_else(|e| panic!("{}: {e}", case.group()));
            case.check(b, &r.point);
            assert_eq!(r.validation.station_policy[0].loss_mode, Some(case.mode()));
            if case.regime == 1 {
                assert!(r.point[&format!("station/{STATION}/pconv")].abs() * b.mva < 1e-7);
                assert!(r.point[&format!("station/{STATION}/iconv")] > 0.);
            }
            let output = powerio::serialize(
                &powerio::PioModule::new(powerio::PioValue::from(r.solution)),
                powerio::Destination::memory("fixed.json").unwrap(),
            )
            .unwrap();
            let powerio::EmittedOutput::Memory { artifacts } = output.output() else {
                panic!()
            };
            assert!(powerio::deserialize(
                powerio::Source::from_memory("fixed.json", artifacts[0].bytes().to_vec()).unwrap()
            )
            .is_ok());
        }
    }
    // Explicit mixed modes on connected and islanded multi-station systems.
    // Fixed-P sinks remain in inverter mode; isolated DC terminals consume idle
    // losses in rectifier mode. No mode is inferred from a solved sign.
    for topology in 0..8 {
        for poles in [1, 2] {
            for b in mesh::bases() {
                let case = pf_connected::Case {
                    topology,
                    direction: 0,
                    poles,
                    control: 0,
                };
                let mut n = case.network(b);
                let mut o = n.ac_dc_equivalent().unwrap().clone();
                let mut modes = Vec::new();
                let mut quadratic = [1.3; 3];
                for (i, s) in o.stations.iter_mut().enumerate() {
                    s.losses.rectifier_quadratic_ohm = 0.3;
                    s.losses.inverter_quadratic_ohm = 1.3;
                    if !s.in_service {
                        continue;
                    }
                    let rec = i == 0 || (i == 2 && matches!(topology, 5 | 6));
                    quadratic[i] = if rec { 0.3 } else { 1.3 };
                    modes.push(AcDcStationLossMode {
                        component: s.component.to_string(),
                        mode: if rec {
                            AcDcValveLossMode::Rectifier
                        } else {
                            AcDcValveLossMode::Inverter
                        },
                    });
                }
                modes.reverse();
                n.set_ac_dc_equivalent(Some(o)).unwrap();
                let model = PreparedAcDcPf::new(
                    AcDcPfInstance::from_network_with_loss_modes(n, fixture::PROFILE, modes)
                        .unwrap(),
                )
                .unwrap();
                let r = model
                    .solve(AcDcPfStart::Declared, None)
                    .unwrap_or_else(|e| panic!("mixed {topology}/{poles}/{b:?}: {e}"));
                assert!(
                    connected::physical_residual_with_losses(
                        case.base(),
                        &connected::physical(b, &r.point),
                        false,
                        quadratic
                    ) < 1e-6
                );
            }
        }
    }
}
#[test]
fn fixed_mode_pf_rejects_wrong_branch_and_stale_mode_starts() {
    let b = optimized::coordinates()[2];
    for rectifier in [false, true] {
        let case = fixture::Case {
            mask: 7,
            rectifier,
            regime: 0,
            poles: 2,
            droop: false,
        };
        let old = case.prepared(b);
        let r = old.solve(AcDcPfStart::Declared, None).unwrap();
        // Operating ratings do not become mode-domain constraints or clipping.
        let mut rated = case.network(b);
        let mut owner = rated.ac_dc_equivalent().unwrap().clone();
        owner.stations[0].valve_active_mw.min = -40.;
        owner.stations[0].valve_active_mw.max = 40.;
        owner.stations[0].valve_current_limit_pu = 0.1 / b.bases().current_ka();
        rated.set_ac_dc_equivalent(Some(owner)).unwrap();
        let limited = PreparedAcDcPf::new(
            AcDcPfInstance::from_network_with_loss_modes(rated, fixture::PROFILE, case.modes())
                .unwrap(),
        )
        .unwrap()
        .solve(AcDcPfStart::Declared, None)
        .unwrap();
        case.check(b, &limited.point);
        assert!(limited
            .limit_violations
            .iter()
            .any(|e| e.name.ends_with("/pconv") && e.violation * b.mva > 40.));
        assert!(limited
            .limit_violations
            .iter()
            .any(|e| e.name.ends_with("/iconv") && e.violation * b.bases().current_ka() > 0.1));
        let mode = if rectifier {
            AcDcValveLossMode::Inverter
        } else {
            AcDcValveLossMode::Rectifier
        };
        let wrong = PreparedAcDcPf::new(
            AcDcPfInstance::from_network_with_loss_modes(
                case.network(b),
                fixture::PROFILE,
                vec![AcDcStationLossMode {
                    component: STATION.into(),
                    mode,
                }],
            )
            .unwrap(),
        )
        .unwrap();
        assert!(wrong
            .initial_point(&AcDcPfStart::Point {
                model_fingerprint: old.manifest().model_fingerprint.clone(),
                values: r.point.clone()
            })
            .is_err());
        assert!(wrong
            .evaluate(&r.point)
            .unwrap()
            .bounds
            .iter()
            .any(|c| c.name.ends_with("/pconv") && c.violation > 0.01));
        assert!(wrong
            .solve(AcDcPfStart::Declared, None)
            .unwrap_err()
            .contains("loss-mode domain"));
    }
}
#[test]
fn fixed_mode_pf_zero_boundary_keeps_curve_and_derivatives() {
    let b = optimized::coordinates()[3];
    for rectifier in [false, true] {
        let c = fixture::Case {
            mask: 0,
            rectifier,
            regime: 1,
            poles: 1,
            droop: true,
        };
        let i = c.intent(b);
        let m = PreparedAcDcPf::new(i.clone()).unwrap();
        let r = m.solve(AcDcPfStart::Declared, None).unwrap();
        let e = ReferenceEquations::from_pf_instance(&i).unwrap();
        for x in [
            r.point.clone(),
            optimized::starts(&c.circuit(), b).remove(1).1,
        ] {
            for d in e.check_derivatives(&x).unwrap().checks {
                assert!(d.max_scaled_error < 1e-6, "{d:?}");
            }
        }
        let other = fixture::Case {
            rectifier: !rectifier,
            ..c
        };
        let other_point = other
            .prepared(b)
            .solve(AcDcPfStart::Declared, None)
            .unwrap()
            .point;
        let delta = (r.point[&format!("station/{STATION}/pdc")]
            - other_point[&format!("station/{STATION}/pdc")])
            * b.mva;
        assert!((delta.abs() - 0.08_f64.powi(2)).abs() < 1e-7);
    }
}
