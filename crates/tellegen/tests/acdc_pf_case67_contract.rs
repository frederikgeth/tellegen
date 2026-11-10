#![cfg(feature = "acopf")]
#[path = "support/acdc_pf_case67_contract.rs"]
mod fixture;
use powerio::{AcDcPfInstance, AcDcPfProfile, BusType};

#[test]
fn case67_pf_reference_is_an_explicit_study_edit_and_existing_profiles_still_refuse() {
    let mut n = fixture::network(0);
    assert!(!n
        .branches()
        .iter()
        .any(|b| b.in_service && (b.from.0 == 67 || b.to.0 == 67)));
    assert!(n.generators().iter().any(|g| g.in_service && g.bus.0 == 67));
    n.buses_mut()
        .iter_mut()
        .find(|b| b.id.0 == 67)
        .unwrap()
        .kind = BusType::Pv;
    assert_eq!(
        serde_json::to_value(n).unwrap(),
        serde_json::to_value(fixture::source()).unwrap()
    );
    for profile in [
        AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1,
        AcDcPfProfile::PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1,
    ] {
        let original = AcDcPfInstance::from_network(fixture::source(), profile)
            .unwrap_err()
            .to_string();
        assert!(original.contains("exactly one declared AC reference"));
        let edited = AcDcPfInstance::from_network(fixture::network(0), profile)
            .unwrap_err()
            .to_string();
        assert!(edited.contains("droop controls are unqualified"));
    }
}

#[test]
fn case67_droop_withdrawal_slope_and_power_units_are_explicit() {
    // Raising Vdc lowers DC withdrawal; neither PCC generation signs nor MW
    // values can be substituted directly in the per-unit row.
    let r = fixture::droop_residual;
    let (pref, vref, k, base) = (500., 0.9947, 0.005, 100.);
    for dv in [-0.01, 0., 0.01] {
        let p = pref / base - dv / k;
        assert!(r(p, vref + dv, pref, vref, k, base).abs() < 1e-12);
        assert!((r(p + 0.01, vref + dv, pref, vref, k, base) - 0.01).abs() < 1e-12);
    }
    assert!((r(5., vref + 0.001, pref, vref, k, base) - 0.2).abs() < 1e-12);
    assert!((r(500., vref, pref, vref, k, base) - 495.).abs() < 1e-12);
}
