//! Directional-loss witnesses, not a production policy or an OPF mode switch.
#![allow(dead_code)]
use super::physical::*;
use num_complex::Complex64 as C;
use powerio::{AcDcOpfInstance, AcDcOpfProfile, BalancedNetwork};
use tellegen::ac_dc::{EquationMode, ReferenceEquations};
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Case {
    pub operating: &'static str,
    pub coefficients: [f64; 2],
    pub selected: &'static str,
    pub mask: u8,
    pub poles: u8,
}
pub fn cases() -> Vec<Case> {
    let mut cases = vec![];
    for coefficients in [[0.3, 1.3], [1.3, 0.3], [1.3, 1.3]] {
        for mask in [0, 7] {
            for poles in [1, 2] {
                for operating in [
                    "forward",
                    "reverse",
                    "pcc_sign_trap",
                    "near_positive",
                    "near_negative",
                    "zero_valve",
                    "zero_current",
                    "dc_idle",
                ] {
                    for selected in ["rectifier", "inverter"] {
                        cases.push(Case {
                            operating,
                            coefficients,
                            selected,
                            mask,
                            poles,
                        });
                    }
                }
            }
        }
    }
    cases
}
impl Case {
    pub fn quadratic(self) -> f64 {
        self.coefficients[usize::from(self.selected == "inverter")]
    }
    pub fn circuit(self) -> Circuit {
        let current = match self.operating {
            "forward" => C::new(0.25, -0.08),
            "reverse" => C::new(-0.25, -0.08),
            "pcc_sign_trap" => C::new(-0.00005, -0.08),
            "near_positive" => C::new(1e-8 / (3.0_f64.sqrt() * 200.), -0.08),
            "near_negative" => C::new(-1e-8 / (3.0_f64.sqrt() * 200.), -0.08),
            "zero_valve" => C::new(0., -0.08),
            "zero_current" => C::new(0., 0.),
            "dc_idle" => {
                let a = 3.0_f64.sqrt() * 200. - LOSS[1];
                C::new(
                    2. * LOSS[0] / (a + (a * a - 4. * self.quadratic() * LOSS[0]).sqrt()),
                    0.,
                )
            }
            _ => panic!("unknown operating state"),
        };
        Circuit::with_current_and_loss(
            current.re.signum(),
            self.poles,
            Elements::from_mask(self.mask),
            current,
            self.quadratic(),
        )
    }
    pub fn network(self, b: Bases) -> BalancedNetwork {
        let mut net = self.circuit().network(b, 0.5);
        // Explicit identities make IR preservation independent of UID synthesis.
        net.buses_mut()[0].uid = Some("11".into());
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        owner.stations[0].losses.rectifier_quadratic_ohm = self.coefficients[0];
        owner.stations[0].losses.inverter_quadratic_ohm = self.coefficients[1];
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        net
    }
    pub fn expected_loss_residual_mw(self) -> f64 {
        (self.quadratic() - self.coefficients[1]) * self.circuit().valve_current.norm_sqr()
    }
    pub fn equations(self, b: Bases) -> ReferenceEquations {
        ReferenceEquations::from_instance(
            &AcDcOpfInstance::from_network(
                self.network(b),
                AcDcOpfProfile::PhysicalCoordinatesAcpVscV1,
            )
            .unwrap(),
            EquationMode::Opf,
        )
        .unwrap()
    }
    // Physical naming by VALVE AC withdrawal, with no arbitrary tie at zero.
    pub fn named_mode(self) -> Option<&'static str> {
        match self.operating {
            "forward" | "near_positive" | "dc_idle" => Some("rectifier"),
            "reverse" | "near_negative" | "pcc_sign_trap" => Some("inverter"),
            _ => None,
        }
    }
}
