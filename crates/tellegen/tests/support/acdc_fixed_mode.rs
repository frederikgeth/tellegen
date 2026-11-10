//! Fixed valve-loss modes on a multi-converter DC mesh; free dispatch within each branch.
#![allow(dead_code)]
use super::{mesh, optimized::Point};
use mesh::{Bases3, Case};
use powerio::{
    AcDcOpfInstance, AcDcOpfProfile, AcDcStationLossMode, AcDcValveLossMode, BalancedNetwork,
};
use tellegen::ac_dc::PreparedAcDcOpf;
pub fn coefficients(case: Case) -> [f64; 2] {
    if case.flags == 0 {
        [0.3, 1.3]
    } else {
        [1.3, 0.3]
    }
}
pub fn modes(case: Case) -> Vec<AcDcStationLossMode> {
    (0..3)
        .map(|i| AcDcStationLossMode {
            component: mesh::station(i),
            mode: if i == case.cheap {
                AcDcValveLossMode::Rectifier
            } else {
                AcDcValveLossMode::Inverter
            },
        })
        .collect()
}
pub fn network(case: Case, b: Bases3) -> BalancedNetwork {
    let mut net = case.network(b);
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    let [rec, inv] = coefficients(case);
    for s in &mut owner.stations {
        s.losses.rectifier_quadratic_ohm = rec;
        s.losses.inverter_quadratic_ohm = inv;
    }
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    net
}
pub fn intent(case: Case, b: Bases3) -> AcDcOpfInstance {
    AcDcOpfInstance::from_network_with_loss_modes(
        network(case, b),
        AcDcOpfProfile::FixedModePhysicalAcpVscV1,
        modes(case),
    )
    .unwrap()
}
pub fn prepared(case: Case, b: Bases3) -> PreparedAcDcOpf {
    PreparedAcDcOpf::new(intent(case, b)).unwrap()
}
pub fn physical_residual(case: Case, x: &Point) -> f64 {
    let [rec, inv] = coefficients(case);
    let mut c = [inv; 3];
    c[case.cheap] = rec;
    let r = mesh::physical_residual_with_losses(case, x, c);
    (0..3)
        .map(|i| {
            let p = x[&format!("station/{}/pconv", mesh::station(i))];
            if i == case.cheap {
                (-p).max(0.)
            } else {
                p.max(0.)
            }
        })
        .fold(r, f64::max)
}
