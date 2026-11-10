#![allow(dead_code)]
//! Study inputs for the case67 PF contract gate; no native PF profile is enabled.
use powerio::{BalancedNetwork, BusType, PioValue};
use powerio_tx::acdc::DcConverterControl;
pub const CASES: [&str; 8] = [
    "reference_67_source_dispatch",
    "droop_power_target",
    "droop_voltage_target",
    "droop_coefficient",
    "droop_only_dc_grid",
    "dc_line_outage",
    "pcc_active_at_67",
    "tight_ratings",
];
pub fn source() -> BalancedNetwork {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc/case67/case67.m");
    let PioValue::BalancedNetwork(net) = powerio::parse(path).unwrap().into_value() else {
        panic!()
    };
    net
}
pub fn network(case: usize) -> BalancedNetwork {
    let mut n = source();
    n.buses_mut()
        .iter_mut()
        .find(|b| b.id.0 == 67)
        .unwrap()
        .kind = BusType::Ref;
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    match case {
        0 => {}
        1 => o.stations[1].settings.dc_withdrawal_target_mw += 25.,
        2 => o.stations[1].settings.dc_voltage_target_pu += 0.002,
        3 => o.stations[1].settings.droop *= 2.,
        4 => o.stations[0].dc_control = DcConverterControl::DcPowerDroop,
        5 => o.branches[0].in_service = false,
        6 => o.stations[8].settings.pcc_withdrawal_mw += 10.,
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
        _ => panic!("unknown case67 contract variant"),
    }
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    n
}
/// Withdrawal-positive source-pu control residual, independently checked against
/// PMACDC's affine row. This study helper is not a production preparation policy.
pub fn droop_residual(pdc: f64, vdc: f64, pref_mw: f64, vref: f64, k: f64, base: f64) -> f64 {
    pdc - pref_mw / base + (vdc - vref) / k
}
