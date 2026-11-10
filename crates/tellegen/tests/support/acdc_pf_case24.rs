#![allow(dead_code)]
//! Explicit case24 control variant and static outage/island scenarios.
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork, BusType, PioValue};
use powerio_tx::acdc::{AcConverterControl, DcConverterControl};
pub const PROFILE: AcDcPfProfile = AcDcPfProfile::PowerModelsAcDc0101AcpVscEqualIncrementV1;
pub const CASES: [&str; 11] = [
    "declared_q_at_215",
    "pcc_active",
    "pcc_reactive",
    "dc_voltage",
    "pcc_voltage",
    "tight_ratings",
    "ac_line_outage",
    "ac_island",
    "dc_island",
    "converter_outage",
    "combined_islands",
];
pub fn source() -> BalancedNetwork {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc/case24/case24_7_jb.m");
    let PioValue::BalancedNetwork(net) = powerio::parse(path).unwrap().into_value() else {
        panic!()
    };
    net
}
pub fn network(case: usize) -> BalancedNetwork {
    let mut net = source();
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    // Original station 6 overlaps the generators' voltage control at bus 215.
    // This explicitly labeled variant uses its retained PCC Q declaration.
    owner.stations[5].ac_control = AcConverterControl::ReactivePowerAtPcc;
    match case {
        0 => {}
        1 => owner.stations[1].settings.pcc_withdrawal_mw += 10.,
        2 => owner.stations[5].settings.pcc_withdrawal_mvar -= 7.,
        3 => owner.stations[0].settings.dc_voltage_target_pu = 1.01,
        4 => {
            net.buses_mut()
                .iter_mut()
                .find(|b| b.id.0 == 204)
                .unwrap()
                .vm = 1.01
        }
        5 => {
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
        6 => {
            let b = net
                .branches_mut()
                .iter_mut()
                .find(|b| b.from.0 == 101 && b.to.0 == 102)
                .unwrap();
            b.in_service = false;
        }
        7 | 8 | 10 => {
            if case != 8 {
                for b in net
                    .branches_mut()
                    .iter_mut()
                    .filter(|b| b.from.0 == 107 || b.to.0 == 107)
                {
                    b.in_service = false;
                }
                net.buses_mut()
                    .iter_mut()
                    .find(|b| b.id.0 == 107)
                    .unwrap()
                    .kind = BusType::Ref;
            }
            if case != 7 {
                // Cut DC 2--3: isolated DC bus 2 now needs its own Vdc controller.
                owner.branches[1].in_service = false;
                owner.stations[1].dc_control = DcConverterControl::DcVoltage;
            }
        }
        9 => owner.stations[4].in_service = false,
        _ => panic!("unknown case24 PF fixture"),
    }
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    net
}
pub fn instance(case: usize) -> AcDcPfInstance {
    AcDcPfInstance::from_network(network(case), PROFILE).unwrap()
}

pub fn refusals() -> Vec<(&'static str, BalancedNetwork, &'static str)> {
    let mut cases = vec![(
        "original_overlapping_voltage",
        source(),
        "overlapping AC voltage",
    )];
    let mut n = network(7);
    n.buses_mut()
        .iter_mut()
        .find(|b| b.id.0 == 107)
        .unwrap()
        .kind = BusType::Pv;
    cases.push((
        "ac_split_missing_reference",
        n,
        "exactly one declared AC reference",
    ));
    let mut n = network(0);
    n.buses_mut()
        .iter_mut()
        .find(|b| b.id.0 == 101)
        .unwrap()
        .kind = BusType::Ref;
    cases.push((
        "duplicate_ac_reference",
        n,
        "exactly one declared AC reference",
    ));
    let mut n = network(8);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.stations[1].dc_control = DcConverterControl::ActivePowerAtPcc;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    cases.push((
        "dc_split_missing_controller",
        n,
        "exactly one declared DC voltage controller",
    ));
    let mut n = network(0);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.stations[1].dc_control = DcConverterControl::DcVoltage;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    cases.push((
        "duplicate_dc_controller",
        n,
        "exactly one declared DC voltage controller",
    ));
    let mut n = network(0);
    n.buses_mut()
        .iter_mut()
        .find(|b| b.id.0 == 213)
        .unwrap()
        .kind = BusType::Pv;
    cases.push((
        "missing_original_ac_reference",
        n,
        "exactly one declared AC reference",
    ));
    let mut n = network(0);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.stations[0].dc_control = DcConverterControl::ActivePowerAtPcc;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    cases.push((
        "missing_original_dc_controller",
        n,
        "exactly one declared DC voltage controller",
    ));
    let mut n = network(7);
    for g in n.generators_mut().iter_mut().filter(|g| g.bus.0 == 107) {
        g.in_service = false;
    }
    cases.push((
        "unsupplied_reference_island",
        n,
        "PF reference needs active generation",
    ));
    let mut n = network(0);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    o.stations[5].ac_bus = powerio::BusId(204);
    o.stations[5].ac_control = AcConverterControl::VoltageAtPcc;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    cases.push(("duplicate_converter_voltage", n, "overlapping AC voltage"));
    cases
}

pub fn larger_refusals() -> Vec<(&'static str, BalancedNetwork, &'static str)> {
    [
        (
            "case39",
            "case39_10_he.m",
            "exactly one declared DC voltage controller",
        ),
        ("case67", "case67.m", "exactly one declared AC reference"),
    ]
    .into_iter()
    .map(|(name, file, error)| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../evals/validation/acdc")
            .join(name)
            .join(file);
        let PioValue::BalancedNetwork(net) = powerio::parse(path).unwrap().into_value() else {
            panic!()
        };
        (name, net, error)
    })
    .collect()
}
