//! Explicit valve-side loss modes; independently constructed phase-domain roots.
#![allow(dead_code)]
use super::{
    optimized::{self, Coordinates, Point},
    pf_physical,
    physical::*,
};
use num_complex::Complex64 as C;
use powerio::{
    AcDcPfInstance, AcDcPfProfile, AcDcStationLossMode, AcDcValveLossMode, BalancedNetwork,
};
use tellegen::ac_dc::PreparedAcDcPf;
pub const PROFILE: AcDcPfProfile =
    AcDcPfProfile::FixedModePhysicalDcDroopSharedVoltageEqualIncrementV1;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Case {
    pub mask: u8,
    pub rectifier: bool,
    pub regime: u8,
    pub poles: u8,
    pub droop: bool,
}
pub fn cases() -> Vec<Case> {
    let mut v = vec![];
    for mask in [0, 3, 7] {
        for rectifier in [false, true] {
            for regime in 0..3 {
                for poles in [1, 2] {
                    for droop in [false, true] {
                        v.push(Case {
                            mask,
                            rectifier,
                            regime,
                            poles,
                            droop,
                        });
                    }
                }
            }
        }
    }
    v
}
impl Case {
    pub fn group(self) -> String {
        format!(
            "mask{}/rectifier{}/regime{}/poles{}/droop{}",
            self.mask, self.rectifier, self.regime, self.poles, self.droop
        )
    }
    pub fn coefficient(self) -> f64 {
        if self.rectifier {
            0.3
        } else {
            1.3
        }
    }
    pub fn mode(self) -> AcDcValveLossMode {
        if self.rectifier {
            AcDcValveLossMode::Rectifier
        } else {
            AcDcValveLossMode::Inverter
        }
    }
    pub fn circuit(self) -> Circuit {
        let direction = if self.rectifier { 1. } else { -1. };
        let real = match self.regime {
            0 => 0.25,
            1 => 0.,
            _ => 1e-6,
        };
        Circuit::with_current_and_loss(
            direction,
            self.poles,
            Elements::from_mask(self.mask),
            C::new(direction * real, -0.08),
            self.coefficient(),
        )
    }
    pub fn network(self, b: Coordinates) -> BalancedNetwork {
        let mut n = pf_physical::network(&self.circuit(), b, self.droop);
        let mut o = n.ac_dc_equivalent().unwrap().clone();
        o.stations[0].losses.rectifier_quadratic_ohm = 0.3;
        o.stations[0].losses.inverter_quadratic_ohm = 1.3;
        n.set_ac_dc_equivalent(Some(o)).unwrap();
        n
    }
    pub fn modes(self) -> Vec<AcDcStationLossMode> {
        vec![AcDcStationLossMode {
            component: STATION.into(),
            mode: self.mode(),
        }]
    }
    pub fn intent(self, b: Coordinates) -> AcDcPfInstance {
        AcDcPfInstance::from_network_with_loss_modes(self.network(b), PROFILE, self.modes())
            .unwrap()
    }
    pub fn prepared(self, b: Coordinates) -> PreparedAcDcPf {
        PreparedAcDcPf::new(self.intent(b)).unwrap()
    }
    pub fn check(self, b: Coordinates, p: &Point) -> f64 {
        let c = self.circuit();
        let physical = b.physical(p);
        let r = optimized::physical_residual_with_loss(&c, &physical, self.coefficient());
        assert!(r < 1e-6, "physical {r}");
        let expected = b.physical(&optimized::starts(&c, b).remove(0).1);
        for (k, v) in &physical {
            assert!(
                (v - expected[k]).abs() < 1e-6,
                "{} {k}: {v} != {}",
                self.group(),
                expected[k]
            );
        }
        let p = physical[&format!("station/{STATION}/pconv")];
        assert!(if self.rectifier {
            p >= -1e-7
        } else {
            p <= 1e-7
        });
        r
    }
}
