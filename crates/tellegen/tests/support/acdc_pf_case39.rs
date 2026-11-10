#![allow(dead_code)]
//! Explicit DC-voltage-control variants; no automatic source repair.
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork, PioValue};
use powerio_tx::acdc::DcConverterControl;
pub const PROFILE: AcDcPfProfile = AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1;
pub const CASES: [&str; 8] = [
    "opf_pg_vdc_converter_1",
    "opf_pg_vdc_converter_8",
    "dc_voltage",
    "pcc_active",
    "pcc_reactive",
    "parallel_dc_line_outage",
    "converter_outage",
    "tight_ratings",
];
pub fn source() -> BalancedNetwork {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc/case39/case39_10_he.m");
    let PioValue::BalancedNetwork(n) = powerio::parse(p).unwrap().into_value() else {
        panic!()
    };
    n
}
pub fn control_only_network(case: usize) -> BalancedNetwork {
    let mut n = source();
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    // This is an explicit study choice, not a preferred controller or a source default.
    o.stations[if case == 1 { 7 } else { 0 }].dc_control = DcConverterControl::DcVoltage;
    match case {
        0 | 1 => {}
        2 => o.stations[0].settings.dc_voltage_target_pu = 1.02,
        3 => o.stations[1].settings.pcc_withdrawal_mw += 10.,
        4 => o.stations[1].settings.pcc_withdrawal_mvar -= 7.,
        5 => o.branches[3].in_service = false, // one of the parallel 2--4 branches
        6 => o.stations[9].in_service = false,
        7 => {
            for s in &mut o.stations {
                s.valve_current_limit_pu = 0.01;
            }
            for g in n.generators_mut() {
                g.pmax = 1.;
                g.pmin = 0.;
                g.qmax = 1.;
                g.qmin = -1.;
            }
        }
        _ => panic!("unknown case39 PF variant"),
    }
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    n
}
/// Fixed PF input dispatch from the already-qualified, pinned source OPF.
/// No OPF voltages, currents, converter dispatch or solved PF state is copied.
pub fn network(case: usize) -> BalancedNetwork {
    let mut n = control_only_network(case);
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc/case39");
    let r: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("reference.json")).unwrap()).unwrap();
    let ids: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("identity-map.json")).unwrap()).unwrap();
    assert_eq!(r["source_sha256"], ids["_source_sha256"]);
    assert_eq!(r["solves"]["opf"]["qualified"], true);
    assert_eq!(
        r["powermodelsacdc_commit"],
        "9ada4082ac087aed649f48fb5f977680b0fe06c7"
    );
    let base = n.base_mva();
    for g in n.generators_mut() {
        let id = ids["gen"][g.uid.as_deref().unwrap()].as_str().unwrap();
        g.pg = r["solves"]["opf"]["solution"]["gen"][id]["pg"]
            .as_f64()
            .unwrap()
            * base;
    }
    n
}
pub fn instance(case: usize) -> AcDcPfInstance {
    AcDcPfInstance::from_network(network(case), PROFILE).unwrap()
}
pub fn refusals() -> Vec<(&'static str, BalancedNetwork, &'static str)> {
    let mut rows = vec![(
        "original_no_dc_controller",
        source(),
        "exactly one declared DC voltage controller",
    )];
    for k in 0..4 {
        let mut n = network(0);
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        let (name, error) = match k {
            0 => {
                o.stations[7].dc_control = DcConverterControl::DcVoltage;
                (
                    "duplicate_dc_controller",
                    "exactly one declared DC voltage controller",
                )
            }
            1 => {
                o.stations[0].in_service = false;
                (
                    "controller_outage",
                    "exactly one declared DC voltage controller",
                )
            }
            2 => {
                o.branches[1].in_service = false;
                (
                    "dc_split_without_local_controller",
                    "exactly one declared DC voltage controller",
                )
            }
            _ => {
                o.stations[0].settings.dc_voltage_target_pu = 0.;
                (
                    "nonpositive_voltage_target",
                    "voltage target must be positive",
                )
            }
        };
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        rows.push((name, n, error));
    }
    rows
}
