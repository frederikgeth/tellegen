#![allow(dead_code)]
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork, PioValue};
use powerio_tx::acdc::{AcConverterControl, DcConverterControl};
pub const PROFILE: AcDcPfProfile = AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1;
pub const CASES: [&str; 8] = [
    "source",
    "pcc_active",
    "pcc_reactive",
    "dc_voltage",
    "pcc_voltage",
    "dual_voltage",
    "tight_ratings",
    "multiple_reference_generators",
];
pub fn network(case: usize) -> BalancedNetwork {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc/case5_3_he.m");
    let PioValue::BalancedNetwork(mut net) = powerio::parse(path).unwrap().into_value() else {
        panic!()
    };
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    match case {
        0 => {}
        1 => owner.stations[0].settings.pcc_withdrawal_mw += 10.,
        2 => owner.stations[0].settings.pcc_withdrawal_mvar -= 7.,
        3 => owner.stations[1].settings.dc_voltage_target_pu = 1.02,
        4 | 5 => {
            owner.stations[0].ac_control = AcConverterControl::VoltageAtPcc;
            owner.stations[0].settings.declared_ac_voltage_pu = 1.08; // retained Vtar is not the reference PF target
            net.buses_mut().iter_mut().find(|b| b.id.0 == 2).unwrap().vm = 1.01;
            if case == 5 {
                owner.stations[0].dc_control = DcConverterControl::DcVoltage;
                owner.stations[0].settings.dc_voltage_target_pu = 1.015;
                owner.stations[1].dc_control = DcConverterControl::ActivePowerAtPcc;
                owner.stations[1].settings.pcc_withdrawal_mw = 10.;
            }
        }
        6 => {
            net.branches_mut()[0].angmin = 5.;
            net.branches_mut()[0].angmax = 5.;
            for s in &mut owner.stations {
                s.valve_current_limit_pu = 0.01;
            }
            for g in net.generators_mut() {
                g.pmax = 1.;
                g.pmin = 0.;
                g.qmax = 1.;
                g.qmin = -1.;
            }
        }
        7 => {
            let mut g = net
                .generators()
                .iter()
                .find(|g| g.bus.0 == 4)
                .unwrap()
                .clone();
            g.uid = Some("pf-second-reference-generator".into());
            g.pg = 25.;
            g.qg = 3.;
            net.generators_mut().push(g);
        }
        _ => panic!("unknown PF fixture"),
    }
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    net
}
pub fn instance(case: usize) -> AcDcPfInstance {
    AcDcPfInstance::from_network(network(case), PROFILE).unwrap()
}
