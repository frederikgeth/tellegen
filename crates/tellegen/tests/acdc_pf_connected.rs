#![cfg(feature = "acopf")]
#[path = "support/acdc_connected.rs"]
mod connected;
#[path = "support/acdc_pf_connected.rs"]
mod fixture;
#[path = "support/acdc_mesh.rs"]
mod mesh;
#[path = "support/acdc_optimized.rs"]
mod optimized;
#[path = "support/acdc_physical.rs"]
mod physical;
use powerio::{AcDcPfInstance, BusType};
use powerio_tx::acdc::DcConverterControl;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf, ReferenceEquations};
#[test]
fn connected_physical_pf_controls_outages_and_equivalent_bases() {
    for case in fixture::cases() {
        for b in mesh::bases() {
            let m = case.prepared(b);
            let r = m
                .solve(AcDcPfStart::Declared, None)
                .unwrap_or_else(|e| panic!("{} {b:?}: {e}", case.group()));
            assert!(case.residual(b, &r.point) < 1e-6);
            assert_eq!(
                m.manifest()
                    .angle_references
                    .iter()
                    .map(|r| r.bus.0)
                    .collect::<Vec<_>>(),
                case.base().expected_anchors()
            );
            assert!(m.manifest().angle_references.iter().all(|r| r.declared));
        }
    }
}
#[test]
fn disconnected_pf_requires_explicit_ac_reference_and_dc_voltage_authority() {
    let b = mesh::bases()[0];
    for topology in [2, 3, 5, 6, 7] {
        let case = fixture::Case {
            topology,
            direction: 0,
            poles: 2,
            control: 0,
        };
        let n = case.network(b);
        if topology != 5 {
            let mut bad = n.clone();
            bad.buses_mut()
                .iter_mut()
                .find(|b| b.id.0 == 13)
                .unwrap()
                .kind = BusType::Pv;
            assert!(AcDcPfInstance::from_network(bad, fixture::PROFILE).is_err());
        }
        let mut bad = n;
        let mut o = bad.ac_dc_equivalent().unwrap().clone();
        for s in &mut o.stations {
            s.dc_control = DcConverterControl::ActivePowerAtPcc;
        }
        bad.set_ac_dc_equivalent(Some(o)).unwrap();
        assert!(AcDcPfInstance::from_network(bad, fixture::PROFILE).is_err());
    }
    let case = fixture::Case {
        topology: 6,
        direction: 0,
        poles: 2,
        control: 1,
    };
    let mut n = case.network(b);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.stations[2].in_service = false;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    assert!(AcDcPfInstance::from_network(n, fixture::PROFILE).is_err());
    let a = fixture::Case {
        topology: 0,
        ..case
    }
    .prepared(b);
    let outage = case.prepared(b);
    assert!(outage
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: a.manifest().model_fingerprint.clone(),
            values: a.initial_point(&AcDcPfStart::Declared).unwrap()
        })
        .is_err());
}
#[test]
fn connected_physical_pf_derivatives_and_corruptions_cover_islands_and_droop() {
    for (topology, control) in [(0, 0), (3, 1), (6, 2)] {
        let c = fixture::Case {
            topology,
            direction: 2,
            poles: 2,
            control,
        };
        let b = mesh::bases()[2];
        let i = c.intent(b);
        let m = PreparedAcDcPf::new(i.clone()).unwrap();
        let r = m.solve(AcDcPfStart::Declared, None).unwrap();
        let eq = ReferenceEquations::from_pf_instance(&i).unwrap();
        for check in eq.check_derivatives(&r.point).unwrap().checks {
            assert!(check.max_scaled_error < 1e-6, "{check:?}");
        }
        for k in [
            format!("dc_bus/{}/vm", mesh::node(2)),
            format!("station/{}/pdc", mesh::station(0)),
            "qg[mesh-generator-2]".into(),
        ] {
            let mut bad = r.point.clone();
            *bad.get_mut(&k).unwrap() += 0.01;
            assert!(m.evaluate(&bad).unwrap().max_violation > 1e-3);
        }
    }
}
