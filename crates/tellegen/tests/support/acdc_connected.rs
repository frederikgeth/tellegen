//! Connected physical AC/DC fixtures and explicitly declared equipment outages.
#![allow(dead_code)]
use super::{mesh, optimized::Point, physical::*};
use mesh::{generator, node, station, Bases3};
use num_complex::Complex64 as C;
use powerio::{
    AcDcAngleReferencePolicy, AcDcOpfInstance, AcDcOpfProfile, BalancedNetwork, Branch, BusId,
    BusType,
};
use tellegen::ac_dc::PreparedAcDcOpf;
pub const AC_Z: C = C::new(1.2, 12.);
pub const AC_B: f64 = 0.00002;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Case {
    pub topology: u8,
    pub cheap: usize,
    pub poles: u8,
}
pub fn cases() -> Vec<Case> {
    let mut v = vec![];
    for topology in 0..8 {
        for cheap in [0, 2] {
            for poles in [1, 2] {
                v.push(Case {
                    topology,
                    cheap,
                    poles,
                });
            }
        }
    }
    v
}
pub fn ac_branch(i: usize) -> String {
    format!("physical-ac-{}", i + 1)
}
impl Case {
    pub fn base(self) -> mesh::Case {
        mesh::Case {
            flags: 0,
            cheap: self.cheap,
            poles: self.poles,
            regime: 0,
        }
    }
    pub fn group(self) -> String {
        format!(
            "topology{}/cheap{}/poles{}",
            self.topology, self.cheap, self.poles
        )
    }
    pub fn ac_active(self, i: usize) -> bool {
        match self.topology {
            1 => i != 0,
            2 | 6 | 7 => i == 0,
            3 => false,
            _ => true,
        }
    }
    pub fn dc_active(self, i: usize) -> bool {
        match self.topology {
            4 => i != 0,
            5 | 6 => i == 0,
            _ => true,
        }
    }
    pub fn station_active(self, i: usize) -> bool {
        self.topology != 7 || i != 2
    }
    pub fn network(self, b: Bases3) -> BalancedNetwork {
        let mut net = self.base().network(b);
        for (i, bus) in net.buses_mut().iter_mut().enumerate() {
            bus.kind = if i == 0 { BusType::Ref } else { BusType::Pv };
            bus.vm = 230. / b.pcc[i];
            bus.uid = Some(bus.id.0.to_string());
        }
        for load in net.loads_mut() {
            load.uid = Some(format!("bus-{}", load.bus.0));
        }
        for i in 0..3 {
            let j = (i + 1) % 3;
            let zbase = b.pcc[j].powi(2) / b.mva;
            // Receiving-side impedance base and normalized unity physical turns.
            let mut line = Branch::new(
                BusId(11 + i),
                BusId(11 + j),
                AC_Z.re / zbase,
                AC_Z.im / zbase,
            );
            line.uid = Some(ac_branch(i));
            line.tap = b.pcc[j] / b.pcc[i];
            line.b = AC_B * zbase;
            line.rate_a = 45.;
            line.rate_b = 45.;
            line.rate_c = 45.;
            line.angmin = -30.;
            line.angmax = 30.;
            line.in_service = self.ac_active(i);
            net.branches_mut().push(line);
        }
        let mut owner = net.ac_dc_equivalent().unwrap().clone();
        for (i, line) in owner.branches.iter_mut().enumerate() {
            line.in_service = self.dc_active(i);
        }
        for (i, s) in owner.stations.iter_mut().enumerate() {
            s.in_service = self.station_active(i);
        }
        net.set_ac_dc_equivalent(Some(owner)).unwrap();
        net
    }
    pub fn intent(self, b: Bases3) -> AcDcOpfInstance {
        AcDcOpfInstance::from_network(self.network(b), AcDcOpfProfile::PhysicalCoordinatesAcpVscV1)
            .unwrap()
            .with_angle_reference_policy(AcDcAngleReferencePolicy::LowestBusIdPerIsland)
    }
    pub fn prepared(self, b: Bases3) -> PreparedAcDcOpf {
        PreparedAcDcOpf::new(self.intent(b)).unwrap()
    }
    pub fn starts(self, b: Bases3) -> Vec<(&'static str, Point)> {
        let model = self.prepared(b);
        let mut x = Point::new();
        for v in &model.manifest().variables {
            let k = &v.name;
            let value = if k.starts_with("pg[") {
                self.base().load(
                    (0..3)
                        .find(|&i| k == &format!("pg[{}]", generator(i)))
                        .unwrap(),
                )
            } else if k.starts_with("qg[") {
                10.
            } else if k.starts_with("dc_bus/") {
                500.
            } else if k.starts_with("vm[") {
                230.
            } else if k.ends_with("/vmfilt") || k.ends_with("/vmconv") {
                let i = (0..3)
                    .find(|&i| k.starts_with(&format!("station/{}/", station(i))))
                    .unwrap();
                if self.base().mask(i) & 1 != 0 {
                    200.
                } else {
                    230.
                }
            } else if k.ends_with("/iconv") {
                0.01
            } else {
                0.
            };
            x.insert(k.clone(), value);
        }
        let mut perturbed = x.clone();
        for (k, v) in &mut perturbed {
            if k.starts_with("vm[")
                || k.ends_with("/vm")
                || k.ends_with("/vmfilt")
                || k.ends_with("/vmconv")
            {
                *v *= 1.01;
            } else if k.starts_with("pg[") || k.starts_with("qg[") {
                *v *= 1.03;
            } else if k.ends_with("/vafilt") || k.ends_with("/vaconv") {
                *v = 0.005;
            }
        }
        // An isolated DC terminal has zero transfer. Initialize its all-bypassed
        // station at the physical idle-loss root, rather than near the singular
        // zero-current branch of the squared current equation.
        if matches!(self.topology, 5 | 6) {
            for point in [&mut x, &mut perturbed] {
                let prefix = format!("station/{}/", station(2));
                let voltage = point[&format!("{prefix}vmconv")];
                let a = 3.0_f64.sqrt() * voltage - LOSS[1];
                let current = 2.0 * LOSS[0] / (a + (a * a - 4.0 * LOSS[2] * LOSS[0]).sqrt());
                let power = 3.0_f64.sqrt() * voltage * current;
                for field in ["pgrid", "ppr_fr", "pconv"] {
                    point.insert(format!("{prefix}{field}"), power);
                }
                point.insert(format!("{prefix}ptf_to"), -power);
                point.insert(format!("{prefix}iconv"), current);
                point.insert(format!("pg[{}]", generator(2)), self.base().load(2) + power);
            }
        }
        vec![
            ("zero-transfer", per_unit(b, &x)),
            ("perturbed", per_unit(b, &perturbed)),
        ]
    }
    pub fn expected_anchors(self) -> Vec<usize> {
        match self.topology {
            2 | 6 | 7 => vec![11, 13],
            3 => vec![11, 12, 13],
            _ => vec![11],
        }
    }
    pub fn variable_count(self) -> usize {
        15 + 14 * (0..3).filter(|&i| self.station_active(i)).count()
            + 2 * (0..3).filter(|&i| self.dc_active(i)).count()
            + 4 * (0..3).filter(|&i| self.ac_active(i)).count()
    }
}
pub fn factor(b: Bases3, k: &str) -> f64 {
    if k.starts_with("ac_branch/") {
        b.mva
    } else {
        b.factor(k)
    }
}
pub fn physical(b: Bases3, x: &Point) -> Point {
    x.iter()
        .map(|(k, v)| (k.clone(), v * factor(b, k)))
        .collect()
}
pub fn per_unit(b: Bases3, x: &Point) -> Point {
    x.iter()
        .map(|(k, v)| (k.clone(), v / factor(b, k)))
        .collect()
}
pub fn bounds(
    model: &PreparedAcDcOpf,
    b: Bases3,
) -> std::collections::BTreeMap<String, [Option<f64>; 2]> {
    model
        .manifest()
        .variables
        .iter()
        .map(|v| {
            (
                v.name.clone(),
                [
                    v.lower.map(|x| x * factor(b, &v.name)),
                    v.upper.map(|x| x * factor(b, &v.name)),
                ],
            )
        })
        .collect()
}
pub fn physical_residual(case: Case, x: &Point) -> f64 {
    physical_residual_with_ratings(case, x, true)
}
pub fn physical_residual_with_ratings(case: Case, x: &Point, ratings: bool) -> f64 {
    physical_residual_with_losses(case, x, ratings, [LOSS[2]; 3])
}
pub fn physical_residual_with_losses(
    case: Case,
    x: &Point,
    ratings: bool,
    quadratic: [f64; 3],
) -> f64 {
    let mut r = vec![];
    let mut ac = [C::new(0., 0.); 3];
    let mut dc = [0.; 3];
    let mut loss = 0.;
    let voltage = |i: usize| {
        C::from_polar(
            x[&format!("vm[{}]", 11 + i)] / 3.0_f64.sqrt(),
            x[&format!("va[{}]", 11 + i)],
        )
    };
    for i in 0..3 {
        let j = (i + 1) % 3;
        if case.ac_active(i) {
            let vf = voltage(i);
            let vt = voltage(j);
            let current = (vf - vt) / AC_Z;
            let sf = 3. * vf * (current + C::new(0., AC_B / 2.) * vf).conj();
            let st = 3. * vt * (-current + C::new(0., AC_B / 2.) * vt).conj();
            let actual = |a: &str, b: &str| {
                C::new(
                    x[&format!("ac_branch/{}/{a}", ac_branch(i))],
                    x[&format!("ac_branch/{}/{b}", ac_branch(i))],
                )
            };
            let pf = actual("pf", "qf");
            let pt = actual("pt", "qt");
            r.extend([
                (sf - pf).norm(),
                (st - pt).norm(),
                if ratings {
                    (pf.norm() - 45.).max(0.)
                } else {
                    0.
                },
                if ratings {
                    (pt.norm() - 45.).max(0.)
                } else {
                    0.
                },
            ]);
            ac[i] += pf;
            ac[j] += pt;
            loss += 3. * AC_Z.re * current.norm_sqr();
        }
        if case.dc_active(i) {
            let from = x[&format!("dc_bus/{}/vm", node(i))];
            let to = x[&format!("dc_bus/{}/vm", node(j))];
            let idc = (from - to) / mesh::RESISTANCE[i];
            let pf = x[&format!("dc_branch/{}/pf", mesh::branch(i))];
            let pt = x[&format!("dc_branch/{}/pt", mesh::branch(i))];
            r.extend([
                (pf - f64::from(case.poles) * from * idc).abs(),
                (pt + f64::from(case.poles) * to * idc).abs(),
            ]);
            dc[i] += pf;
            dc[j] += pt;
            loss += f64::from(case.poles) * mesh::RESISTANCE[i] * idc * idc;
        }
        if case.station_active(i) {
            let s = |f: &str| x[&format!("station/{}/{f}", station(i))];
            let ph = |v: f64, a: f64| C::from_polar(v / 3.0_f64.sqrt(), a);
            let vp = voltage(i);
            let vf = ph(s("vmfilt"), s("vafilt"));
            let vc = ph(s("vmconv"), s("vaconv"));
            let e = Elements::from_mask(case.base().mask(i));
            let current = (C::new(s("pconv"), s("qconv")) / (3. * vc)).conj();
            let it = current
                + if e.filter {
                    C::new(0., FILTER_SIEMENS) * vf
                } else {
                    C::new(0., 0.)
                };
            let turns = if e.transformer { TURNS_RATIO } else { 1. };
            let zt = if e.transformer {
                TRANSFORMER_OHM
            } else {
                C::new(0., 0.)
            };
            let zr = if e.reactor {
                REACTOR_OHM
            } else {
                C::new(0., 0.)
            };
            let sloss = LOSS[0] + LOSS[1] * current.norm() + quadratic[i] * current.norm_sqr();
            r.extend([
                (vf - vc - zr * current).norm(),
                (vp - turns * (vf + zt * it)).norm(),
                (C::new(s("pgrid"), s("qgrid")) - 3. * vp * (it / turns).conj()).norm(),
                (C::new(s("ptf_to"), s("qtf_to")) + 3. * vf * it.conj()).norm(),
                (C::new(s("ppr_fr"), s("qpr_fr")) - 3. * vf * current.conj()).norm(),
                (s("iconv") - current.norm()).abs(),
                (s("pconv") + s("pdc") - sloss).abs(),
            ]);
            ac[i] += C::new(s("pgrid"), s("qgrid"));
            dc[i] += s("pdc");
            loss += sloss + 3. * zt.re * it.norm_sqr() + 3. * zr.re * current.norm_sqr();
        }
    }
    for i in 0..3 {
        r.push(
            (C::new(
                x[&format!("pg[{}]", generator(i))] - case.base().load(i),
                x[&format!("qg[{}]", generator(i))] - 10.,
            ) - ac[i])
                .norm(),
        );
        r.push(dc[i].abs());
    }
    r.push(
        ((0..3)
            .map(|i| x[&format!("pg[{}]", generator(i))] - case.base().load(i))
            .sum::<f64>()
            - loss)
            .abs(),
    );
    r.into_iter().fold(0., f64::max)
}
