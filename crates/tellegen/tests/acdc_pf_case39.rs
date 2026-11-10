#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_case39.rs"]
mod fixture;
use powerio::{AcDcPfInstance, BusType};
use powerio_tx::acdc::DcConverterControl;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};

#[test]
fn case39_voltage_controller_choice_is_explicit_and_preserves_source_settings() {
    let original = serde_json::to_value(fixture::source()).unwrap();
    for case in [0, 1] {
        let mut n = fixture::control_only_network(case);
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        let selected = o
            .stations
            .iter_mut()
            .filter(|s| s.dc_control == DcConverterControl::DcVoltage)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        for s in selected {
            s.dc_control = DcConverterControl::ActivePowerAtPcc;
        }
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        assert_eq!(serde_json::to_value(n).unwrap(), original);
    }
    for (name, n, expected) in fixture::refusals() {
        let e = AcDcPfInstance::from_network(n, fixture::PROFILE)
            .unwrap_err()
            .to_string();
        assert!(e.contains(expected), "{name}: {e}");
    }
    let a = PreparedAcDcPf::new(fixture::instance(0)).unwrap();
    let b = PreparedAcDcPf::new(fixture::instance(1)).unwrap();
    assert!(b
        .initial_point(&AcDcPfStart::Point {
            model_fingerprint: a.manifest().model_fingerprint.clone(),
            values: a.initial_point(&AcDcPfStart::Declared).unwrap()
        })
        .is_err());
}
#[test]
fn case39_explicit_pf_controls_balance_dc_losses_and_report_ratings() {
    for case in 0..fixture::CASES.len() {
        let instance = fixture::instance(case);
        let net = instance.network().clone();
        let original = serde_json::to_value(&net).unwrap();
        let model = PreparedAcDcPf::new(instance).unwrap();
        let r = model
            .solve(AcDcPfStart::Declared, None)
            .unwrap_or_else(|e| panic!("{}: {e}", fixture::CASES[case]));
        assert!(r.validation.max_violation < 1e-9);
        assert_eq!(
            serde_json::to_value(r.solution.network()).unwrap(),
            original
        );
        let owner = net.ac_dc_equivalent().unwrap();
        let mut converter_total = 0.;
        let mut branch_loss = 0.;
        for s in owner.stations.iter().filter(|s| s.in_service) {
            let point = |field| r.point[&format!("station/{}/{field}", s.component)];
            assert!(
                (point("qgrid") * net.base_mva() - s.settings.pcc_withdrawal_mvar).abs() < 1e-7
            );
            if s.dc_control == DcConverterControl::DcVoltage {
                assert!(
                    (r.point[&format!("dc_bus/{}/vm", s.dc_bus)] - s.settings.dc_voltage_target_pu)
                        .abs()
                        < 1e-9
                );
                assert!(
                    (point("pgrid") * net.base_mva() - s.settings.pcc_withdrawal_mw).abs() > 1.
                );
            } else {
                assert!(
                    (point("pgrid") * net.base_mva() - s.settings.pcc_withdrawal_mw).abs() < 1e-7
                );
            }
            converter_total += point("pdc");
        }
        for b in owner.branches.iter().filter(|b| b.in_service) {
            let loss = r.point[&format!("dc_branch/{}/pf", b.component)]
                + r.point[&format!("dc_branch/{}/pt", b.component)];
            assert!(loss >= -1e-9);
            branch_loss += loss;
        }
        assert!(branch_loss > 0.);
        assert!((converter_total + branch_loss).abs() < 1e-8);
        for g in net.generators().iter().filter(|g| g.in_service) {
            if net.buses().iter().find(|b| b.id == g.bus).unwrap().kind != BusType::Ref {
                assert!(
                    (r.point[&format!("pg[{}]", g.uid.as_deref().unwrap())] * net.base_mva()
                        - g.pg)
                        .abs()
                        < 1e-7
                );
            }
        }
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
        if case == 7 {
            assert!(r
                .limit_violations
                .iter()
                .any(|v| v.name.ends_with("/iconv")));
        }
    }
}
