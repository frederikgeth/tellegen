//! Explicit physical fixture conversion; never an implicit imported-data rebase.
#![allow(dead_code)]
use super::{optimized, physical::*};
use optimized::{Coordinates, Point};
use powerio::{AcDcPfInstance, AcDcPfProfile, BalancedNetwork};
use powerio_tx::acdc::DcConverterControl;
use tellegen::ac_dc::{AcDcPfStart, PreparedAcDcPf};
pub const PROFILE: AcDcPfProfile =
    AcDcPfProfile::PhysicalCoordinatesDcDroopSharedVoltageEqualIncrementV1;
pub const DROOP_KV_PER_MW: f64 = 0.2;
pub const DROOP_REFERENCE_KV: f64 = 498.;
pub fn network(c: &Circuit, b: Coordinates, droop: bool) -> BalancedNetwork {
    let mut n = optimized::network(c, b);
    let mut o = n.ac_dc_equivalent().unwrap().clone();
    let s = &mut o.stations[0];
    // Deliberately distinct unused curve: this profile retains Cinv in both directions.
    s.losses.rectifier_quadratic_ohm = 2. * LOSS[2];
    s.dc_control = if droop {
        DcConverterControl::DcPowerDroop
    } else {
        DcConverterControl::DcVoltage
    };
    s.settings.dc_voltage_target_pu = if droop {
        DROOP_REFERENCE_KV
    } else {
        c.dc_from_kv
    } / b.dc_from_kv;
    s.settings.dc_withdrawal_target_mw = -c.dc_from_mw
        + if droop {
            (c.dc_from_kv - DROOP_REFERENCE_KV) / DROOP_KV_PER_MW
        } else {
            0.
        };
    s.settings.droop = DROOP_KV_PER_MW * b.mva / b.dc_from_kv;
    n.set_ac_dc_equivalent(Some(o)).unwrap();
    n
}
pub fn prepared(c: &Circuit, b: Coordinates, droop: bool) -> PreparedAcDcPf {
    PreparedAcDcPf::new(AcDcPfInstance::from_network(network(c, b, droop), PROFILE).unwrap())
        .unwrap()
}
pub fn starts(
    c: &Circuit,
    b: Coordinates,
    model: &PreparedAcDcPf,
) -> Vec<(&'static str, AcDcPfStart)> {
    vec![
        ("declared", AcDcPfStart::Declared),
        (
            "perturbed",
            AcDcPfStart::Point {
                model_fingerprint: model.manifest().model_fingerprint.clone(),
                values: optimized::starts(c, b).remove(1).1,
            },
        ),
    ]
}
pub fn check(c: &Circuit, b: Coordinates, droop: bool, x: &Point) -> f64 {
    let physical = b.physical(x);
    let residual = optimized::physical_residual(c, &physical);
    assert!(residual < 1e-6, "physical residual {residual}");
    let p = physical[&format!("station/{STATION}/pdc")];
    let v = physical[&format!("dc_bus/{DC_FROM}/vm")];
    let control = if droop {
        p + c.dc_from_mw - (c.dc_from_kv - DROOP_REFERENCE_KV) / DROOP_KV_PER_MW
            + (v - DROOP_REFERENCE_KV) / DROOP_KV_PER_MW
    } else {
        v - c.dc_from_kv
    };
    assert!(control.abs() < 1e-6, "physical control {control}");
    let expected = b.physical(&optimized::starts(c, b).remove(0).1);
    for (key, value) in &physical {
        assert!(
            (value - expected[key]).abs() < 1e-6,
            "{key}: {value} != {}",
            expected[key]
        );
    }
    residual.max(control.abs())
}
