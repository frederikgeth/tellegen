//! Physical qualification cases; the production source-coordinate models are unchanged.
use super::physical::*;
use powerio::BalancedNetwork;
use std::collections::BTreeMap;

pub struct Case {
    pub kind: &'static str,
    pub circuit: Circuit,
    pub bases: Bases,
    pub dc_to_base_kv: f64,
}
impl Case {
    pub fn data(&self) -> (BalancedNetwork, BTreeMap<String, f64>) {
        let mut net = self.circuit.network(self.bases, 5.0);
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        let bus = &mut owner.buses[1];
        bus.base_kv = self.dc_to_base_kv;
        bus.initial_voltage_pu = self.circuit.dc_to_kv / self.dc_to_base_kv;
        bus.voltage_limits_pu.min = 450.0 / self.dc_to_base_kv;
        bus.voltage_limits_pu.max = 550.0 / self.dc_to_base_kv;
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        let mut point = self.circuit.point(self.bases);
        point.insert(
            format!("dc_bus/{DC_TO}/vm"),
            self.circuit.dc_to_kv / self.dc_to_base_kv,
        );
        (net, point)
    }
    pub fn expected_violations(&self) -> Vec<String> {
        match self.kind {
            "qualified" => vec![],
            "unequal_pcc_bypass" => vec![format!("station/{STATION}/transformer/vm_bypass")],
            "unequal_dc_bases" => vec![
                format!("dc_branch/{DC_BRANCH}/pf"),
                format!("dc_branch/{DC_BRANCH}/pt"),
            ],
            _ => panic!("unknown physical qualification case"),
        }
    }
}
pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for mask in 0..8 {
        let elements = Elements::from_mask(mask);
        for direction in [-1.0, 1.0] {
            for poles in [1, 2] {
                for mut b in bases() {
                    // An ideal physical bypass requires a common voltage coordinate.
                    if !elements.transformer {
                        b.pcc_kv = b.converter_kv;
                    }
                    out.push(Case {
                        kind: "qualified",
                        circuit: Circuit::with_elements(direction, poles, elements),
                        bases: b,
                        dc_to_base_kv: b.dc_kv,
                    });
                }
                for mva in [50.0, 100.0, 250.0] {
                    if !elements.transformer {
                        let b = Bases {
                            mva,
                            pcc_kv: 230.0,
                            converter_kv: 400.0,
                            dc_kv: 500.0,
                        };
                        out.push(Case {
                            kind: "unequal_pcc_bypass",
                            circuit: Circuit::with_elements(direction, poles, elements),
                            bases: b,
                            dc_to_base_kv: b.dc_kv,
                        });
                    }
                    // Fully enabled and fully bypassed stations isolate the DC mismatch.
                    if mask == 0 || mask == 7 {
                        let b = Bases {
                            mva,
                            pcc_kv: if elements.transformer { 230.0 } else { 200.0 },
                            converter_kv: 200.0,
                            dc_kv: 500.0,
                        };
                        for dc_to_base_kv in [250.0, 1000.0] {
                            out.push(Case {
                                kind: "unequal_dc_bases",
                                circuit: Circuit::with_elements(direction, poles, elements),
                                bases: b,
                                dc_to_base_kv,
                            });
                        }
                    }
                }
            }
        }
    }
    out
}
