//! Analytical three-phase circuit in kV, kA, ohms, siemens and MW/MVAr.
//! No production per-unit branch/transformer formula is used by this oracle.
#![allow(dead_code)]
use num_complex::Complex64 as C;
use powerio::{BalancedNetwork, Bus, BusId, BusType, ComponentId, GenCost, Generator};
use powerio_tx::acdc::*;
use std::collections::BTreeMap;

pub const TRANSFORMER_OHM: C = C::new(1.2, 8.0);
pub const REACTOR_OHM: C = C::new(0.7, 4.0);
pub const FILTER_SIEMENS: f64 = 0.0001;
pub const TURNS_RATIO: f64 = 230.0 / 200.0;
pub const DC_OHM: f64 = 10.0;
pub const LOSS: [f64; 3] = [0.4, 0.7, 1.3];
pub const STATION: &str = "ac_dc_converter_station/31";
pub const DC_FROM: &str = "dc_equivalent_bus/21";
pub const DC_TO: &str = "dc_equivalent_bus/22";
pub const DC_BRANCH: &str = "dc_equivalent_branch/41";

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Bases {
    pub mva: f64,
    pub pcc_kv: f64,
    pub converter_kv: f64,
    pub dc_kv: f64,
}
impl Bases {
    pub fn current_ka(self) -> f64 {
        self.mva / (3.0_f64.sqrt() * self.converter_kv)
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Elements {
    pub transformer: bool,
    pub reactor: bool,
    pub filter: bool,
}
impl Elements {
    pub const ALL: Self = Self {
        transformer: true,
        reactor: true,
        filter: true,
    };
    pub fn from_mask(mask: u8) -> Self {
        Self {
            transformer: mask & 1 != 0,
            reactor: mask & 2 != 0,
            filter: mask & 4 != 0,
        }
    }
}

pub struct Circuit {
    pub elements: Elements,
    pub direction: f64,
    pub poles: u8,
    pub pcc: C,
    pub filter: C,
    pub valve: C,
    pub valve_current: C,
    pub transformer_current: C,
    pub pcc_power: C,
    pub transformer_to: C,
    pub reactor_from: C,
    pub valve_power: C,
    pub valve_loss_mw: f64,
    pub dc_from_kv: f64,
    pub dc_to_kv: f64,
    pub dc_current_ka: f64,
    pub dc_from_mw: f64,
    pub dc_to_mw: f64,
}
impl Circuit {
    pub fn new(direction: f64, poles: u8) -> Self {
        Self::with_elements(direction, poles, Elements::ALL)
    }
    pub fn with_elements(direction: f64, poles: u8, elements: Elements) -> Self {
        // Phase-to-neutral RMS voltage and per-phase current. The loss curve
        // uses AC RMS current magnitude in kA (its coefficients are total MW).
        let valve = C::new(200.0 / 3.0_f64.sqrt(), 0.0);
        let valve_current = C::new(direction * 0.25, -0.08);
        let filter = if elements.reactor {
            valve + REACTOR_OHM * valve_current
        } else {
            valve
        };
        let transformer_current = if elements.filter {
            valve_current + C::new(0.0, FILTER_SIEMENS) * filter
        } else {
            valve_current
        };
        let turns = if elements.transformer {
            TURNS_RATIO
        } else {
            1.0
        };
        let pcc = if elements.transformer {
            turns * (filter + TRANSFORMER_OHM * transformer_current)
        } else {
            filter
        };
        // Choose the physical AC reference frame; this changes no power/current.
        let rotation = C::from_polar(1.0, -pcc.arg());
        let (pcc, filter, valve, valve_current, transformer_current) = (
            pcc * rotation,
            filter * rotation,
            valve * rotation,
            valve_current * rotation,
            transformer_current * rotation,
        );
        let pcc_power = 3.0 * pcc * (transformer_current / turns).conj();
        let transformer_to = -3.0 * filter * transformer_current.conj();
        let reactor_from = 3.0 * filter * valve_current.conj();
        let valve_power = 3.0 * valve * valve_current.conj();
        let current = valve_current.norm();
        let valve_loss_mw = LOSS[0] + LOSS[1] * current + LOSS[2] * current * current;
        let dc_from_mw = valve_power.re - valve_loss_mw;
        let dc_from_kv = 500.0; // each symmetric pole's voltage
        let dc_current_ka = dc_from_mw / (f64::from(poles) * dc_from_kv);
        let dc_to_kv = dc_from_kv - DC_OHM * dc_current_ka;
        let dc_to_mw = -f64::from(poles) * dc_to_kv * dc_current_ka;
        Self {
            elements,
            direction,
            poles,
            pcc,
            filter,
            valve,
            valve_current,
            transformer_current,
            pcc_power,
            transformer_to,
            reactor_from,
            valve_power,
            valve_loss_mw,
            dc_from_kv,
            dc_to_kv,
            dc_current_ka,
            dc_from_mw,
            dc_to_mw,
        }
    }
    pub fn network(&self, b: Bases, current_limit_ka: f64) -> BalancedNetwork {
        let mut bus = Bus::new(BusId(11), BusType::Ref, b.pcc_kv);
        bus.vm = self.pcc.norm() * 3.0_f64.sqrt() / b.pcc_kv;
        bus.vmin = 180.0 / b.pcc_kv;
        bus.vmax = 300.0 / b.pcc_kv;
        let mut generator = Generator::new(bus.id);
        generator.uid = Some("physical-generator".into());
        generator.pg = self.pcc_power.re;
        generator.qg = self.pcc_power.im;
        generator.pmin = -150.0;
        generator.pmax = 150.0;
        generator.qmin = -150.0;
        generator.qmax = 150.0;
        generator.vg = bus.vm;
        generator.mbase = b.mva;
        generator.cost = Some(GenCost::new(2, 0.0, 0.0, vec![0.02, 3.0, 1.0]));
        let mut net =
            BalancedNetwork::in_memory("analytical-physical-acdc", b.mva, vec![bus], vec![]);
        net.generators_mut().push(generator);
        let id = |kind, local| ComponentId::new(kind, local).unwrap();
        let dc_from = id("dc_equivalent_bus", "21");
        let dc_to = id("dc_equivalent_bus", "22");
        let dc_bus = |component, external_id, voltage, withdrawal_mw| DcEquivalentBus {
            component,
            external_id,
            source_grid: 1,
            base_kv: b.dc_kv,
            withdrawal_mw,
            initial_voltage_pu: voltage / b.dc_kv,
            voltage_limits_pu: AcDcLimits {
                min: 450.0 / b.dc_kv,
                max: 550.0 / b.dc_kv,
            },
            capacitance_pu: 0.0,
            area: None,
        };
        let zbase = b.converter_kv.powi(2) / b.mva;
        let series = |z: C, enabled| ConverterSeries {
            enabled,
            resistance_pu: z.re / zbase,
            reactance_pu: z.im / zbase,
        };
        let equipment = BalancedAcDcEquipment {
            base_mva: b.mva,
            pole_count: self.poles,
            buses: vec![
                dc_bus(dc_from.clone(), 21, self.dc_from_kv, 0.0),
                dc_bus(dc_to.clone(), 22, self.dc_to_kv, -self.dc_to_mw),
            ],
            branches: vec![DcEquivalentBranch {
                component: id("dc_equivalent_branch", "41"),
                from: dc_from.clone(),
                to: dc_to,
                resistance_pu: DC_OHM * b.mva / b.dc_kv.powi(2),
                inductance_pu_per_second: 0.0,
                capacitance_pu_seconds: 0.0,
                ratings_mw: [150.0; 3],
                in_service: true,
            }],
            stations: vec![AcDcConverterStation {
                component: id("ac_dc_converter_station", "31"),
                ac_bus: BusId(11),
                dc_bus: dc_from,
                technology: ConverterTechnology::Vsc,
                ac_control: AcConverterControl::ReactivePowerAtPcc,
                dc_control: DcConverterControl::ActivePowerAtPcc,
                ac_base_kv: b.converter_kv,
                transformer: series(TRANSFORMER_OHM, self.elements.transformer),
                // Physical turns ratio = tap_pu * PCC_base / converter_base.
                transformer_tap_ratio: TURNS_RATIO * b.converter_kv / b.pcc_kv,
                filter_enabled: self.elements.filter,
                filter_susceptance_pu: FILTER_SIEMENS * zbase,
                reactor: series(REACTOR_OHM, self.elements.reactor),
                valve_voltage_pu: AcDcLimits {
                    min: 180.0 / b.converter_kv,
                    max: 240.0 / b.converter_kv,
                },
                valve_current_limit_pu: current_limit_ka / b.current_ka(),
                valve_active_mw: AcDcLimits {
                    min: -100.0,
                    max: 100.0,
                },
                valve_reactive_mvar: AcDcLimits {
                    min: -100.0,
                    max: 100.0,
                },
                losses: AcCurrentLosses {
                    constant_mw: LOSS[0],
                    linear_kv: LOSS[1],
                    rectifier_quadratic_ohm: LOSS[2],
                    inverter_quadratic_ohm: LOSS[2],
                },
                settings: ConverterSettings {
                    pcc_withdrawal_mw: self.pcc_power.re,
                    pcc_withdrawal_mvar: self.pcc_power.im,
                    declared_ac_voltage_pu: self.pcc.norm() * 3.0_f64.sqrt() / b.pcc_kv,
                    droop: 0.0,
                    dc_withdrawal_target_mw: -self.dc_from_mw,
                    dc_voltage_target_pu: self.dc_from_kv / b.dc_kv,
                    dc_voltage_deadband_pu: 0.0,
                },
                in_service: true,
            }],
        };
        net.set_ac_dc_equivalent(Some(equipment)).unwrap();
        net
    }
    pub fn point(&self, b: Bases) -> BTreeMap<String, f64> {
        let mut x = BTreeMap::from([
            ("vm[11]".into(), self.pcc.norm() * 3.0_f64.sqrt() / b.pcc_kv),
            ("va[11]".into(), 0.0),
            ("pg[physical-generator]".into(), self.pcc_power.re / b.mva),
            ("qg[physical-generator]".into(), self.pcc_power.im / b.mva),
            (format!("dc_bus/{DC_FROM}/vm"), self.dc_from_kv / b.dc_kv),
            (format!("dc_bus/{DC_TO}/vm"), self.dc_to_kv / b.dc_kv),
            (format!("dc_branch/{DC_BRANCH}/pf"), self.dc_from_mw / b.mva),
            (format!("dc_branch/{DC_BRANCH}/pt"), self.dc_to_mw / b.mva),
        ]);
        for (field, value) in [
            ("pgrid", self.pcc_power.re),
            ("qgrid", self.pcc_power.im),
            ("ptf_to", self.transformer_to.re),
            ("qtf_to", self.transformer_to.im),
            ("ppr_fr", self.reactor_from.re),
            ("qpr_fr", self.reactor_from.im),
            ("pconv", self.valve_power.re),
            ("qconv", self.valve_power.im),
            ("pdc", -self.dc_from_mw),
        ] {
            x.insert(format!("station/{STATION}/{field}"), value / b.mva);
        }
        for (field, value) in [
            (
                "vmfilt",
                self.filter.norm() * 3.0_f64.sqrt() / b.converter_kv,
            ),
            ("vafilt", self.filter.arg()),
            (
                "vmconv",
                self.valve.norm() * 3.0_f64.sqrt() / b.converter_kv,
            ),
            ("vaconv", self.valve.arg()),
            ("iconv", self.valve_current.norm() / b.current_ka()),
        ] {
            x.insert(format!("station/{STATION}/{field}"), value);
        }
        x
    }
    pub fn objective(&self) -> f64 {
        0.02 * self.pcc_power.re.powi(2) + 3.0 * self.pcc_power.re + 1.0
    }
}

pub fn bases() -> Vec<Bases> {
    [50.0, 100.0, 250.0]
        .into_iter()
        .flat_map(|mva| {
            [
                (230.0, 200.0, 500.0),
                (400.0, 345.0, 250.0),
                (230.0, 400.0, 800.0),
            ]
            .into_iter()
            .map(move |(pcc_kv, converter_kv, dc_kv)| Bases {
                mva,
                pcc_kv,
                converter_kv,
                dc_kv,
            })
        })
        .collect()
}
