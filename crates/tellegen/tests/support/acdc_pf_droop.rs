#![allow(dead_code)]
#[path = "acdc_pf_case67_contract.rs"]
mod contract;
pub use contract::{network, source, CASES};
use powerio::{AcDcPfInstance, AcDcPfProfile};
pub const PROFILE: AcDcPfProfile =
    AcDcPfProfile::PowerModelsAcDc0101AcpVscDcDroopSharedVoltageEqualIncrementV1;
pub fn instance(case: usize) -> AcDcPfInstance {
    AcDcPfInstance::from_network(network(case), PROFILE).unwrap()
}
pub fn refusals() -> Vec<(&'static str, powerio::BalancedNetwork, &'static str)> {
    use powerio_tx::acdc::DcConverterControl;
    let mut cases = vec![(
        "original_missing_ac_reference",
        source(),
        "exactly one declared AC reference",
    )];
    for k in 0..10 {
        let mut n = network(0);
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        let (name, error) = match k {
            0 => {
                o.stations[1].settings.droop = 0.;
                ("zero_slope", "slope must be positive")
            }
            1 => {
                o.stations[1].settings.droop = -0.005;
                ("negative_slope", "slope must be positive")
            }
            2 => {
                o.stations[1].settings.droop = 1e-320;
                ("nonfinite_inverse_slope", "finite reciprocal")
            }
            3 => {
                o.stations[1].settings.dc_voltage_target_pu = 0.;
                (
                    "nonpositive_droop_voltage",
                    "voltage target must be positive",
                )
            }
            4 => {
                o.stations[1].settings.dc_voltage_deadband_pu = 0.001;
                ("deadband", "deadband is unqualified")
            }
            5 => {
                o.stations[1].dc_control = DcConverterControl::ValveAcPowerDroop;
                ("valve_power_droop", "droop controls are unqualified")
            }
            6 => {
                o.stations[1].dc_control = DcConverterControl::DcVoltage;
                ("duplicate_fixed_voltage", "PF DC island")
            }
            7 => {
                for s in &mut o.stations {
                    s.dc_control = DcConverterControl::ActivePowerAtPcc;
                }
                ("no_dc_authority", "PF DC island")
            }
            8 => {
                for s in &mut o.stations {
                    s.dc_control = DcConverterControl::ActivePowerAtPcc;
                }
                o.stations[1].dc_control = DcConverterControl::DcPowerDroop;
                o.stations[1].in_service = false;
                ("last_droop_controller_outage", "PF DC island")
            }
            _ => {
                o.branches[10].in_service = false;
                ("uncontrolled_dc_island", "PF DC island")
            }
        };
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        cases.push((name, n, error));
    }
    cases
}
