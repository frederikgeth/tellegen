#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_case24.rs"]
mod fixture;
use powerio::{AcDcPfInstance, BusType};
use powerio_tx::acdc::{AcConverterControl, DcConverterControl};
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};

#[test]
fn case24_pf_control_and_island_variants_preserve_declarations() {
    for case in 0..fixture::CASES.len() {
        let instance = fixture::instance(case);
        let original = serde_json::to_value(instance.network()).unwrap();
        let net = instance.network().clone();
        let model = PreparedAcDcPf::new(instance).unwrap();
        let r = model
            .solve(AcDcPfStart::Declared, None)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture::CASES[case]));
        assert!(r.validation.max_violation < 1e-9);
        assert_eq!(
            serde_json::to_value(r.solution.network()).unwrap(),
            original
        );
        assert_eq!(
            model.manifest().angle_references.len(),
            if case == 7 || case == 10 { 4 } else { 3 }
        );
        // Independent checks of declared controls and generator allocation.
        for s in net
            .ac_dc_equivalent()
            .unwrap()
            .stations
            .iter()
            .filter(|s| s.in_service)
        {
            let point = |field| r.point[&format!("station/{}/{field}", s.component)];
            if s.dc_control == DcConverterControl::ActivePowerAtPcc {
                assert!(
                    (point("pgrid") * net.base_mva() - s.settings.pcc_withdrawal_mw).abs() < 1e-7
                );
            } else {
                assert!(
                    (r.point[&format!("dc_bus/{}/vm", s.dc_bus)] - s.settings.dc_voltage_target_pu)
                        .abs()
                        < 1e-9
                );
            }
            if s.ac_control == AcConverterControl::ReactivePowerAtPcc {
                assert!(
                    (point("qgrid") * net.base_mva() - s.settings.pcc_withdrawal_mvar).abs() < 1e-7
                );
            } else {
                let bus = net.buses().iter().find(|b| b.id == s.ac_bus).unwrap();
                assert!((r.point[&format!("vm[{}]", bus.id.0)] - bus.vm).abs() < 1e-9);
            }
        }
        for bus in net.buses() {
            let gens: Vec<_> = net
                .generators()
                .iter()
                .filter(|g| g.in_service && g.bus == bus.id)
                .collect();
            if let Some(first) = gens.first() {
                for g in &gens {
                    let output = |g: &powerio::Generator, f| {
                        r.point[&format!("{f}[{}]", g.uid.as_deref().unwrap())] * net.base_mva()
                    };
                    assert!((output(g, "qg") - g.qg - output(first, "qg") + first.qg).abs() < 1e-7);
                    if bus.kind == BusType::Ref {
                        assert!(
                            (output(g, "pg") - g.pg - output(first, "pg") + first.pg).abs() < 1e-7
                        );
                    } else {
                        assert!((output(g, "pg") - g.pg).abs() < 1e-7);
                    }
                }
            }
        }
        for b in net.branches().iter().filter(|b| !b.in_service) {
            assert!(!r
                .point
                .contains_key(&format!("ac_branch/{}/pf", b.uid.as_deref().unwrap())));
        }
        let owner = net.ac_dc_equivalent().unwrap();
        for b in owner.branches.iter().filter(|b| !b.in_service) {
            assert!(!r
                .point
                .contains_key(&format!("dc_branch/{}/pf", b.component)));
        }
        for s in owner.stations.iter().filter(|s| !s.in_service) {
            assert!(!r
                .point
                .contains_key(&format!("station/{}/pgrid", s.component)));
        }
        if case == 8 || case == 10 {
            assert!(r.current_retractions > 0);
            let i = r.point["station/ac_dc_converter_station/2/iconv"];
            assert!(i > 0. && i < 0.02);
            assert!(r.point["station/ac_dc_converter_station/2/pdc"].abs() < 1e-10);
        }
        if case == 5 {
            assert!(r
                .limit_violations
                .iter()
                .any(|v| v.name.ends_with("/iconv")));
        }
        assert!(r
            .validation
            .bus_type_policy
            .iter()
            .any(|b| b.bus_id == 123 && b.declared == BusType::Pq && b.effective == BusType::Pv));
    }
}

#[test]
fn case24_rejects_ambiguous_controls_and_unreferenced_islands_before_solving() {
    for (name, net, expected) in fixture::refusals()
        .into_iter()
        .chain(fixture::larger_refusals())
    {
        let error = AcDcPfInstance::from_network(net, fixture::PROFILE)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{name}: {error}");
    }
}

#[test]
fn case24_outage_invalidates_pre_outage_starts() {
    let before = PreparedAcDcPf::new(fixture::instance(0)).unwrap();
    let after = PreparedAcDcPf::new(fixture::instance(10)).unwrap();
    assert!(after
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: before.manifest().model_fingerprint.clone(),
            values: before.initial_point(&AcDcPfStart::Declared).unwrap(),
        })
        .unwrap_err()
        .contains("different PF model"));
}
