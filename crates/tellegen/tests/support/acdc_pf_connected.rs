//! Explicit PF controls and references on physical connected/outage fixtures.
#![allow(dead_code)]
use super::{
    connected,
    mesh::{self, Bases3},
    optimized::Point,
};
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork, BusType};
use powerio_tx::acdc::{AcConverterControl, DcConverterControl};
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};
pub const PROFILE: AcDcPfProfile =
    AcDcPfProfile::PhysicalCoordinatesDcDroopSharedVoltageEqualIncrementV1;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Case {
    pub topology: u8,
    pub direction: usize,
    pub poles: u8,
    pub control: u8,
}
pub fn cases() -> Vec<Case> {
    let mut cases = vec![];
    for topology in 0..8 {
        for direction in [0, 2] {
            for poles in [1, 2] {
                for control in 0..3 {
                    cases.push(Case {
                        topology,
                        direction,
                        poles,
                        control,
                    });
                }
            }
        }
    }
    cases
}
impl Case {
    pub fn base(self) -> connected::Case {
        connected::Case {
            topology: self.topology,
            cheap: self.direction,
            poles: self.poles,
        }
    }
    pub fn group(self) -> String {
        format!(
            "topology{}/direction{}/poles{}/control{}",
            self.topology, self.direction, self.poles, self.control
        )
    }
    pub fn network(self, b: Bases3) -> BalancedNetwork {
        let mut n = self.base().network(b);
        // Explicit study edit. PF must never borrow numerical OPF anchors.
        let refs = self.base().expected_anchors();
        for bus in n.buses_mut() {
            bus.kind = if refs.contains(&bus.id.0) {
                BusType::Ref
            } else {
                BusType::Pv
            };
        }
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        for (i, s) in o.stations.iter_mut().enumerate() {
            let anchor = i == 0 || (matches!(self.topology, 5 | 6) && i == 2);
            s.ac_control = if i == 1 {
                AcConverterControl::VoltageAtPcc
            } else {
                AcConverterControl::ReactivePowerAtPcc
            };
            s.settings.pcc_withdrawal_mw = if i == self.direction { 20. } else { -10. };
            s.settings.pcc_withdrawal_mvar = 5.;
            s.settings.declared_ac_voltage_pu = 230. / b.pcc[i];
            s.settings.dc_voltage_target_pu = 500. / b.dc[i];
            s.settings.dc_withdrawal_target_mw = if i == self.direction { -20. } else { 10. };
            s.settings.droop = 0.2 * b.mva / b.dc[i];
            s.dc_control = if self.control != 1 && anchor {
                DcConverterControl::DcVoltage
            } else if self.control == 0 {
                DcConverterControl::ActivePowerAtPcc
            } else {
                DcConverterControl::DcPowerDroop
            };
        }
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        n
    }
    pub fn intent(self, b: Bases3) -> AcDcPfInstance {
        AcDcPfInstance::from_network(self.network(b), PROFILE).unwrap()
    }
    pub fn prepared(self, b: Bases3) -> PreparedAcDcPf {
        PreparedAcDcPf::new(self.intent(b)).unwrap()
    }
    pub fn starts(self, b: Bases3, m: &PreparedAcDcPf) -> Vec<(&'static str, AcDcPfStart)> {
        let mut x = m.initial_point(&AcDcPfStart::Declared).unwrap();
        for (key, v) in &mut x {
            if key.starts_with("vm[")
                || key.ends_with("/vm")
                || key.ends_with("/vmfilt")
                || key.ends_with("/vmconv")
            {
                *v *= 1.01;
            } else if key.ends_with("/vafilt") || key.ends_with("/vaconv") {
                *v += 0.005;
            } else if key.starts_with("pg[") || key.starts_with("qg[") {
                *v *= 0.97;
            }
        }
        let _ = b;
        vec![
            ("declared", AcDcPfStart::Declared),
            (
                "perturbed",
                AcDcPfStart::Point {
                    model_fingerprint: m.manifest().model_fingerprint.clone(),
                    values: x,
                },
            ),
        ]
    }
    pub fn residual(self, b: Bases3, x: &Point) -> f64 {
        let physical = connected::physical(b, x);
        let mut r = connected::physical_residual_with_ratings(self.base(), &physical, false);
        let n = self.network(b);
        let o = n.ac_dc_equivalent().unwrap();
        for (i, s) in o.stations.iter().enumerate().filter(|(_, s)| s.in_service) {
            let p = physical[&format!("station/{}/pdc", mesh::station(i))];
            let v = physical[&format!("dc_bus/{}/vm", mesh::node(i))];
            let control = match s.dc_control {
                DcConverterControl::DcVoltage => v - 500.,
                DcConverterControl::DcPowerDroop => {
                    p - s.settings.dc_withdrawal_target_mw + (v - 500.) / 0.2
                }
                _ => {
                    physical[&format!("station/{}/pgrid", mesh::station(i))]
                        - s.settings.pcc_withdrawal_mw
                }
            };
            r = r.max(control.abs());
            let q = physical[&format!("station/{}/qgrid", mesh::station(i))];
            let reactive = if i == 1 {
                physical[&format!("qg[{}]", mesh::generator(i))] - 10. + q - 5.
            } else {
                q - 5.
            };
            r = r.max(reactive.abs());
        }
        for (i, bus) in n.buses().iter().enumerate() {
            r = r.max((physical[&format!("vm[{}]", bus.id.0)] - 230.).abs());
            if bus.kind == BusType::Ref {
                r = r.max(physical[&format!("va[{}]", bus.id.0)].abs());
            } else {
                r = r.max(
                    (physical[&format!("pg[{}]", mesh::generator(i))] - self.base().base().load(i))
                        .abs(),
                );
            }
        }
        r
    }
}
