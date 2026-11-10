//! Three AC islands connected through three VSCs and a physical DC triangle.
#![allow(dead_code)]
use super::{optimized, physical::*};
use optimized::Point;
use powerio::{
    AcDcOpfInstance, AcDcOpfProfile, BalancedNetwork, BusId, ComponentId, GenCost, Load,
};
use tellegen::ac_dc::PreparedAcDcOpf;
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Bases3 {
    pub mva: f64,
    pub pcc: [f64; 3],
    pub converter: [f64; 3],
    pub dc: [f64; 3],
}
pub fn bases() -> Vec<Bases3> {
    vec![
        Bases3 {
            mva: 100.,
            pcc: [230.; 3],
            converter: [200.; 3],
            dc: [500.; 3],
        },
        Bases3 {
            mva: 50.,
            pcc: [230.; 3],
            converter: [200.; 3],
            dc: [500.; 3],
        },
        Bases3 {
            mva: 100.,
            pcc: [400., 230., 345.],
            converter: [345., 400., 200.],
            dc: [250., 1000., 800.],
        },
        Bases3 {
            mva: 250.,
            pcc: [230., 400., 200.],
            converter: [400., 200., 345.],
            dc: [800., 250., 500.],
        },
    ]
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Case {
    pub flags: usize,
    pub cheap: usize,
    pub poles: u8,
    pub regime: usize,
}
pub fn cases() -> Vec<Case> {
    let mut v = vec![];
    for flags in 0..2 {
        for cheap in [0, 2] {
            for poles in [1, 2] {
                for regime in 0..3 {
                    v.push(Case {
                        flags,
                        cheap,
                        poles,
                        regime,
                    });
                }
            }
        }
    }
    v
}
pub const RESISTANCE: [f64; 3] = [10., 15., 20.];
pub fn station(i: usize) -> String {
    format!("ac_dc_converter_station/{}", 31 + i)
}
pub fn node(i: usize) -> String {
    format!("dc_equivalent_bus/{}", 21 + i)
}
pub fn branch(i: usize) -> String {
    format!("dc_equivalent_branch/{}", 41 + i)
}
pub fn generator(i: usize) -> String {
    format!("mesh-generator-{}", i + 1)
}
impl Case {
    pub fn group(self) -> String {
        format!(
            "flags{}/cheap{}/poles{}/regime{}",
            self.flags, self.cheap, self.poles, self.regime
        )
    }
    pub fn mask(self, i: usize) -> u8 {
        [[7, 3, 0], [0, 4, 7]][self.flags][i]
    }
    pub fn load(self, i: usize) -> f64 {
        if i == self.cheap {
            10.
        } else {
            65.
        }
    }
    pub fn linear_cost(self, i: usize) -> f64 {
        if i == self.cheap {
            1.
        } else {
            4. + i as f64
        }
    }
    pub fn current_limit(self, i: usize) -> f64 {
        if self.regime == 2 && i == self.cheap {
            0.18
        } else {
            0.5
        }
    }
    pub fn rating(self, i: usize) -> f64 {
        if self.regime == 1 && i == self.cheap {
            20.
        } else {
            200.
        }
    }
    pub fn network(self, b: Bases3) -> BalancedNetwork {
        let mut net =
            BalancedNetwork::in_memory("physical-three-terminal-mesh", b.mva, vec![], vec![]);
        let mut owner = None;
        for (i, resistance) in RESISTANCE.iter().enumerate() {
            let c = Circuit::with_elements(1., self.poles, Elements::from_mask(self.mask(i)));
            let part = c.network(
                Bases {
                    mva: b.mva,
                    pcc_kv: b.pcc[i],
                    converter_kv: b.converter[i],
                    dc_kv: b.dc[i],
                },
                self.current_limit(i),
            );
            let mut bus = part.buses()[0].clone();
            bus.id = BusId(11 + i);
            bus.uid = None;
            bus.vm = 230. / b.pcc[i];
            let mut g = part.generators()[0].clone();
            g.bus = bus.id;
            g.uid = Some(generator(i));
            g.pmin = 0.;
            g.pmax = 200.;
            g.pg = self.load(i);
            g.qg = 10.;
            g.cost = Some(GenCost::new(
                2,
                0.,
                0.,
                vec![0.002, self.linear_cost(i), 1.],
            ));
            net.loads_mut().push(Load::new(bus.id, self.load(i), 10.));
            net.buses_mut().push(bus);
            net.generators_mut().push(g);
            let e = part.ac_dc_equivalent().unwrap();
            let o = owner.get_or_insert_with(|| {
                let mut o = e.clone();
                o.stations.clear();
                o.buses.clear();
                o.branches.clear();
                o
            });
            let mut dc = e.buses[0].clone();
            dc.component = ComponentId::new("dc_equivalent_bus", (21 + i).to_string()).unwrap();
            dc.external_id = (21 + i) as u32;
            dc.withdrawal_mw = 0.;
            dc.initial_voltage_pu = 500. / b.dc[i];
            o.buses.push(dc.clone());
            let mut s = e.stations[0].clone();
            s.component =
                ComponentId::new("ac_dc_converter_station", (31 + i).to_string()).unwrap();
            s.ac_bus = BusId(11 + i);
            s.dc_bus = dc.component;
            s.valve_active_mw.min = -150.;
            s.valve_active_mw.max = 150.;
            o.stations.push(s);
            let mut line = e.branches[0].clone();
            line.component =
                ComponentId::new("dc_equivalent_branch", (41 + i).to_string()).unwrap();
            line.from = ComponentId::new("dc_equivalent_bus", (21 + i).to_string()).unwrap();
            line.to =
                ComponentId::new("dc_equivalent_bus", (21 + (i + 1) % 3).to_string()).unwrap();
            line.resistance_pu = *resistance * b.mva / b.dc[i].powi(2);
            line.ratings_mw = [self.rating(i); 3];
            o.branches.push(line);
        }
        net.set_ac_dc_equivalent(owner).unwrap();
        net
    }
    pub fn prepared(self, b: Bases3) -> PreparedAcDcOpf {
        PreparedAcDcOpf::new(
            AcDcOpfInstance::from_network(
                self.network(b),
                AcDcOpfProfile::PhysicalCoordinatesAcpVscV1,
            )
            .unwrap(),
        )
        .unwrap()
    }
    pub fn starts(self, b: Bases3) -> Vec<(&'static str, Point)> {
        let model = self.prepared(b);
        let mut physical = Point::new();
        for v in &model.manifest().variables {
            let k = &v.name;
            let x = if k.starts_with("pg[") {
                let i = (0..3)
                    .find(|&i| k == &format!("pg[{}]", generator(i)))
                    .unwrap();
                self.load(i)
            } else if k.starts_with("qg[") {
                10.
            } else if k.starts_with("dc_bus/") {
                500.
            } else if k.starts_with("vm[") {
                let i = (0..3).find(|&i| k == &format!("vm[{}]", 11 + i)).unwrap();
                if self.mask(i) & 1 != 0 {
                    230.
                } else {
                    200.
                }
            } else if k.ends_with("/vmfilt") || k.ends_with("/vmconv") {
                200.
            } else if k.ends_with("/iconv") {
                0.01
            } else {
                0.
            };
            physical.insert(k.clone(), x);
        }
        let mut perturbed = physical.clone();
        for (k, v) in &mut perturbed {
            if k.starts_with("vm[")
                || k.ends_with("/vm")
                || k.ends_with("/vmfilt")
                || k.ends_with("/vmconv")
            {
                *v *= 1.02;
            } else if k.starts_with("pg[") || k.starts_with("qg[") {
                *v *= 1.03;
            } else if k.ends_with("/vafilt") || k.ends_with("/vaconv") {
                *v = 0.01;
            }
        }
        vec![
            ("zero-flow", b.per_unit(&physical)),
            ("perturbed", b.per_unit(&perturbed)),
        ]
    }
    pub fn cost(self, x: &Point) -> f64 {
        (0..3)
            .map(|i| {
                let p = x[&format!("pg[{}]", generator(i))];
                0.002 * p * p + self.linear_cost(i) * p + 1.
            })
            .sum()
    }
}
impl Bases3 {
    pub fn factor(self, k: &str) -> f64 {
        for i in 0..3 {
            if k == format!("vm[{}]", 11 + i) {
                return self.pcc[i];
            }
            if k == format!("va[{}]", 11 + i) {
                return 1.;
            }
            if k == format!("dc_bus/{}/vm", node(i)) {
                return self.dc[i];
            }
            if let Some(field) = k.strip_prefix(&format!("station/{}/", station(i))) {
                return match field {
                    "vmfilt" | "vmconv" => self.converter[i],
                    "vafilt" | "vaconv" => 1.,
                    "iconv" => self.mva / (3.0_f64.sqrt() * self.converter[i]),
                    _ => self.mva,
                };
            }
        }
        assert!(
            k.starts_with("pg[") || k.starts_with("qg[") || k.starts_with("dc_branch/"),
            "unknown {k}"
        );
        self.mva
    }
    pub fn physical(self, x: &Point) -> Point {
        x.iter()
            .map(|(k, v)| (k.clone(), v * self.factor(k)))
            .collect()
    }
    pub fn per_unit(self, x: &Point) -> Point {
        x.iter()
            .map(|(k, v)| (k.clone(), v / self.factor(k)))
            .collect()
    }
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
                    v.lower.map(|x| x * b.factor(&v.name)),
                    v.upper.map(|x| x * b.factor(&v.name)),
                ],
            )
        })
        .collect()
}

/// Independent phase-domain and per-pole DC equations, in physical units.
pub fn physical_residual(case: Case, x: &Point) -> f64 {
    use num_complex::Complex64 as C;
    let mut residuals = vec![];
    let mut dc_kcl = [0.; 3];
    let mut total_loss = 0.;
    for i in 0..3 {
        let get = |f: &str| x[&format!("station/{}/{f}", station(i))];
        let phasor = |v: f64, a: f64| C::from_polar(v / 3.0_f64.sqrt(), a);
        let vp = phasor(x[&format!("vm[{}]", 11 + i)], x[&format!("va[{}]", 11 + i)]);
        let vf = phasor(get("vmfilt"), get("vafilt"));
        let vc = phasor(get("vmconv"), get("vaconv"));
        let e = Elements::from_mask(case.mask(i));
        let current = (C::new(get("pconv"), get("qconv")) / (3. * vc)).conj();
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
        let loss = LOSS[0] + LOSS[1] * current.norm() + LOSS[2] * current.norm_sqr();
        residuals.extend([
            (vf - vc - zr * current).norm(),
            (vp - turns * (vf + zt * it)).norm(),
            (C::new(get("pgrid"), get("qgrid")) - 3. * vp * (it / turns).conj()).norm(),
            (C::new(get("ptf_to"), get("qtf_to")) + 3. * vf * it.conj()).norm(),
            (C::new(get("ppr_fr"), get("qpr_fr")) - 3. * vf * current.conj()).norm(),
            (get("iconv") - current.norm()).abs(),
            (get("pconv") + get("pdc") - loss).abs(),
            (x[&format!("pg[{}]", generator(i))] - case.load(i) - get("pgrid")).abs(),
            (x[&format!("qg[{}]", generator(i))] - 10. - get("qgrid")).abs(),
        ]);
        dc_kcl[i] += get("pdc");
        total_loss += loss + 3. * zt.re * it.norm_sqr() + 3. * zr.re * current.norm_sqr();
        let j = (i + 1) % 3;
        let from = x[&format!("dc_bus/{}/vm", node(i))];
        let to = x[&format!("dc_bus/{}/vm", node(j))];
        let idc = (from - to) / RESISTANCE[i];
        let pf = x[&format!("dc_branch/{}/pf", branch(i))];
        let pt = x[&format!("dc_branch/{}/pt", branch(i))];
        residuals.extend([
            (pf - f64::from(case.poles) * from * idc).abs(),
            (pt + f64::from(case.poles) * to * idc).abs(),
        ]);
        dc_kcl[i] += pf;
        dc_kcl[j] += pt;
        total_loss += f64::from(case.poles) * RESISTANCE[i] * idc * idc;
    }
    residuals.extend(dc_kcl.map(f64::abs));
    residuals.push(
        ((0..3)
            .map(|i| x[&format!("pg[{}]", generator(i))] - case.load(i))
            .sum::<f64>()
            - total_loss)
            .abs(),
    );
    residuals.into_iter().fold(0., f64::max)
}
pub fn binding(case: Case, x: &Point) {
    if case.regime == 1 {
        let k = branch(case.cheap);
        let v = x[&format!("dc_branch/{k}/pf")]
            .abs()
            .max(x[&format!("dc_branch/{k}/pt")].abs());
        assert!((v - 20.).abs() < 1e-5, "line limit not binding: {v}");
    }
    if case.regime == 2 {
        let v = x[&format!("station/{}/iconv", station(case.cheap))];
        assert!((v - 0.18).abs() < 1e-6, "current limit not binding: {v}");
    }
    assert!(x[&format!("station/{}/pgrid", station(case.cheap))] > 1.);
    assert!((0..3)
        .filter(|&i| i != case.cheap)
        .any(|i| x[&format!("station/{}/pgrid", station(i))] < -1.));
}
