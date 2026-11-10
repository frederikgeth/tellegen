#![allow(dead_code)]
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork, BusId, PioValue};
use powerio_tx::acdc::AcConverterControl;
pub const PROFILE: AcDcPfProfile =
    AcDcPfProfile::PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1;
pub const CASES: [&str; 8] = [
    "case24_original",
    "case5_unchanged",
    "mixed_at_pv",
    "mixed_at_reference",
    "converter_only",
    "mixed_multiple",
    "inactive_voltage_controller",
    "tight_ratings",
];
pub fn network(case: usize) -> BalancedNetwork {
    let file = if case == 0 {
        "case24/case24_7_jb.m"
    } else {
        "case5_3_he.m"
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc")
        .join(file);
    let PioValue::BalancedNetwork(mut net) = powerio::parse(path).unwrap().into_value() else {
        panic!()
    };
    if case <= 1 {
        return net;
    }
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    let bus = match case {
        3 => 4,
        4 => 2,
        _ => 1,
    };
    owner.stations[0].ac_bus = BusId(bus);
    owner.stations[0].ac_control = AcConverterControl::VoltageAtPcc;
    owner.stations[0].settings.pcc_withdrawal_mvar = 15.;
    // Vtar deliberately differs; every controller uses the common source bus Vm.
    owner.stations[0].settings.declared_ac_voltage_pu = 1.08;
    if case >= 4 {
        owner.stations[2].ac_bus = BusId(bus);
        owner.stations[2].ac_control = AcConverterControl::VoltageAtPcc;
        owner.stations[2].settings.pcc_withdrawal_mvar = -5.;
        owner.stations[2].settings.declared_ac_voltage_pu = 0.94;
    }
    for (j, g) in net
        .generators_mut()
        .iter_mut()
        .filter(|g| g.bus.0 == bus)
        .enumerate()
    {
        g.qg = if j == 0 { 9. } else { -3. };
    }
    if case == 6 {
        owner.stations[2].in_service = false;
    }
    if case == 7 {
        for s in &mut owner.stations {
            s.valve_current_limit_pu = 0.01;
        }
        for g in net.generators_mut() {
            g.qmax = 1.;
            g.qmin = -1.;
        }
    }
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    net
}
pub fn instance(case: usize) -> AcDcPfInstance {
    AcDcPfInstance::from_network(network(case), PROFILE).unwrap()
}
