//! Explicit fixture conversion only; this is not a general network rebase API.
#![allow(dead_code)]
use super::physical::*;
use powerio::{AcDcOpfInstance, AcDcOpfProfile, BalancedNetwork, GenCost};
use std::collections::BTreeMap;
use tellegen::ac_dc::{AcDcOpfStart, PreparedAcDcOpf};
pub type Point = BTreeMap<String, f64>;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Coordinates {
    pub mva: f64,
    pub pcc_kv: f64,
    pub converter_kv: f64,
    pub dc_from_kv: f64,
    pub dc_to_kv: f64,
}
impl Coordinates {
    pub fn bases(self) -> Bases {
        Bases {
            mva: self.mva,
            pcc_kv: self.pcc_kv,
            converter_kv: self.converter_kv,
            dc_kv: self.dc_from_kv,
        }
    }
    // Every native coordinate must be accounted for; unknown quantities refuse.
    pub fn factor(self, key: &str) -> f64 {
        if key == "vm[11]" {
            self.pcc_kv
        } else if key == format!("dc_bus/{DC_FROM}/vm") {
            self.dc_from_kv
        } else if key == format!("dc_bus/{DC_TO}/vm") {
            self.dc_to_kv
        } else if key.ends_with("/vmfilt") || key.ends_with("/vmconv") {
            self.converter_kv
        } else if key.ends_with("/iconv") {
            self.bases().current_ka()
        } else if key == "va[11]" || key.ends_with("/vafilt") || key.ends_with("/vaconv") {
            1.0
        } else if key.starts_with("pg[")
            || key.starts_with("qg[")
            || key.starts_with("dc_branch/")
            || [
                "/pgrid", "/qgrid", "/ptf_to", "/qtf_to", "/ppr_fr", "/qpr_fr", "/pconv", "/qconv",
                "/pdc",
            ]
            .iter()
            .any(|s| key.ends_with(s))
        {
            self.mva
        } else {
            panic!("unaccounted coordinate {key}")
        }
    }
    pub fn physical(self, point: &Point) -> Point {
        point
            .iter()
            .map(|(k, v)| (k.clone(), v * self.factor(k)))
            .collect()
    }
    pub fn per_unit(self, point: &Point) -> Point {
        point
            .iter()
            .map(|(k, v)| (k.clone(), v / self.factor(k)))
            .collect()
    }
}
pub fn coordinates() -> Vec<Coordinates> {
    [
        (100., 230., 200., 500., 500.),
        (50., 230., 200., 500., 500.),
        (100., 400., 345., 250., 1000.),
        (250., 230., 400., 800., 250.),
    ]
    .into_iter()
    .map(
        |(mva, pcc_kv, converter_kv, dc_from_kv, dc_to_kv)| Coordinates {
            mva,
            pcc_kv,
            converter_kv,
            dc_from_kv,
            dc_to_kv,
        },
    )
    .collect()
}
pub fn network(c: &Circuit, b: Coordinates) -> BalancedNetwork {
    let mut net = c.network(b.bases(), 0.3);
    // Strictly increasing cost over [-150,150] MW, in either flow direction.
    net.generators_mut()[0].cost = Some(GenCost::new(2, 0., 0., vec![0.002, 3., 1.]));
    let mut owner = net.ac_dc_equivalent().unwrap().clone();
    let to = &mut owner.buses[1];
    let ratio = to.base_kv / b.dc_to_kv;
    to.base_kv = b.dc_to_kv;
    to.initial_voltage_pu *= ratio;
    to.voltage_limits_pu.min *= ratio;
    to.voltage_limits_pu.max *= ratio;
    net.set_ac_dc_equivalent(Some(owner)).unwrap();
    net
}
pub fn prepared(c: &Circuit, b: Coordinates) -> PreparedAcDcOpf {
    PreparedAcDcOpf::new(
        AcDcOpfInstance::from_network(network(c, b), AcDcOpfProfile::PhysicalCoordinatesAcpVscV1)
            .unwrap(),
    )
    .unwrap()
}
pub fn starts(c: &Circuit, b: Coordinates) -> Vec<(&'static str, Point)> {
    let mut analytical = c.point(b.bases());
    analytical.insert(format!("dc_bus/{DC_TO}/vm"), c.dc_to_kv / b.dc_to_kv);
    let mut perturbed = b.physical(&analytical);
    for (key, v) in &mut perturbed {
        if key.starts_with("vm[")
            || key.ends_with("/vm")
            || key.ends_with("/vmfilt")
            || key.ends_with("/vmconv")
        {
            *v *= 1.02;
        } else if key.ends_with("/iconv") {
            *v *= 0.95;
        } else if key.ends_with("/vafilt") || key.ends_with("/vaconv") {
            *v += 0.01;
        } else if key != "va[11]" {
            *v *= 0.97;
        }
    }
    vec![
        ("analytical", analytical),
        ("perturbed", b.per_unit(&perturbed)),
    ]
}
pub fn start(model: &PreparedAcDcOpf, point: Point) -> AcDcOpfStart {
    AcDcOpfStart::Point {
        model_fingerprint: model.manifest().model_fingerprint.clone(),
        values: point,
    }
}
pub fn cost(point: &Point) -> f64 {
    let p = point["pg[physical-generator]"];
    0.002 * p * p + 3. * p + 1.
}
pub fn cost_close(a: f64, b: f64) {
    assert!(
        (a - b).abs() < 1e-6 + 1e-8 * a.abs().max(b.abs()),
        "objective {a} != {b}"
    );
}

/// Direct phase-domain physical equations, independent of polar per-unit tapes.
/// Returns the largest absolute residual in kV/kA/MW/MVAr (not a scaled KKT norm).
pub fn physical_residual(c: &Circuit, x: &Point) -> f64 {
    physical_residual_with_loss(c, x, LOSS[2])
}
pub fn physical_residual_with_loss(c: &Circuit, x: &Point, quadratic_ohm: f64) -> f64 {
    use num_complex::Complex64 as C;
    let s = |field: &str| x[&format!("station/{STATION}/{field}")];
    let voltage = |magnitude: f64, angle: f64| C::from_polar(magnitude / 3.0_f64.sqrt(), angle);
    let vp = voltage(x["vm[11]"], x["va[11]"]);
    let vf = voltage(s("vmfilt"), s("vafilt"));
    let vc = voltage(s("vmconv"), s("vaconv"));
    let power = C::new(s("pconv"), s("qconv"));
    let current = (power / (3. * vc)).conj();
    let it = current
        + if c.elements.filter {
            C::new(0., FILTER_SIEMENS) * vf
        } else {
            C::new(0., 0.)
        };
    let turns = if c.elements.transformer {
        TURNS_RATIO
    } else {
        1.
    };
    let expected_filter = vc
        + if c.elements.reactor {
            REACTOR_OHM * current
        } else {
            C::new(0., 0.)
        };
    let expected_pcc = turns
        * (vf
            + if c.elements.transformer {
                TRANSFORMER_OHM * it
            } else {
                C::new(0., 0.)
            });
    let from = x[&format!("dc_bus/{DC_FROM}/vm")];
    let to = x[&format!("dc_bus/{DC_TO}/vm")];
    let idc = (from - to) / DC_OHM;
    let pf = x[&format!("dc_branch/{DC_BRANCH}/pf")];
    let pt = x[&format!("dc_branch/{DC_BRANCH}/pt")];
    let loss = LOSS[0] + LOSS[1] * current.norm() + quadratic_ohm * current.norm_sqr();
    [
        (vf - expected_filter).norm(),
        (vp - expected_pcc).norm(),
        (C::new(s("pgrid"), s("qgrid")) - 3. * vp * (it / turns).conj()).norm(),
        (C::new(s("ptf_to"), s("qtf_to")) + 3. * vf * it.conj()).norm(),
        (C::new(s("ppr_fr"), s("qpr_fr")) - 3. * vf * current.conj()).norm(),
        (s("iconv") - current.norm()).abs(),
        (s("pconv") + s("pdc") - loss).abs(),
        (pf - f64::from(c.poles) * from * idc).abs(),
        (pt + f64::from(c.poles) * to * idc).abs(),
        (pf + s("pdc")).abs(),
        (pt - c.dc_to_mw).abs(),
        (x["pg[physical-generator]"] - s("pgrid")).abs(),
        (x["qg[physical-generator]"] - s("qgrid")).abs(),
    ]
    .into_iter()
    .fold(0., f64::max)
}

pub fn physical_bounds(
    model: &PreparedAcDcOpf,
    b: Coordinates,
) -> BTreeMap<String, [Option<f64>; 2]> {
    model
        .manifest()
        .variables
        .iter()
        .map(|v| {
            (
                v.name.clone(),
                [
                    v.lower.map(|x| x * b.factor(&v.name)),
                    v.upper.map(|x| x * b.factor(&v.name)),
                ],
            )
        })
        .collect()
}
