use powerio::{BalancedNetwork, PioValue};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub fn load(dir: &Path) -> (BalancedNetwork, Value, Value) {
    let read =
        |name| serde_json::from_slice::<Value>(&std::fs::read(dir.join(name)).unwrap()).unwrap();
    let map = read("identity-map.json");
    let source = map["_source_file"].as_str().unwrap_or("case5_3_he.m");
    let module = powerio::parse(dir.join(source)).expect("parse pinned case through PowerIO");
    let PioValue::BalancedNetwork(net) = module.into_value() else {
        panic!("balanced case required")
    };
    (net, read("reference.json"), map)
}

// Explicit fixture identity crosswalk; never interpret native row positions as
// external bus IDs. P/Q are already passive withdrawals in reference solutions.
pub fn point(net: &BalancedNetwork, solution: &Value, map: &Value) -> BTreeMap<String, f64> {
    let mut x = BTreeMap::new();
    let mut put = |native: String, table: &str, id: &str, field: &str| {
        let reference_id = map[table][id]
            .as_str()
            .expect("explicit identity crosswalk entry");
        let value = solution[table][reference_id][field]
            .as_f64()
            .expect("reference scalar");
        assert!(x.insert(native, value).is_none());
    };
    for b in net.buses() {
        for field in ["vm", "va"] {
            put(
                format!("{field}[{}]", b.id.0),
                "bus",
                &b.id.0.to_string(),
                field,
            );
        }
    }
    for g in net.generators().iter().filter(|g| g.in_service) {
        let id = g.uid.as_deref().unwrap();
        for field in ["pg", "qg"] {
            put(format!("{field}[{id}]"), "gen", id, field);
        }
    }
    for b in net.branches().iter().filter(|b| b.in_service) {
        let id = b.uid.as_deref().unwrap();
        for field in ["pf", "qf", "pt", "qt"] {
            put(format!("ac_branch/{id}/{field}"), "branch", id, field);
        }
    }
    let owner = net.ac_dc_equivalent().unwrap();
    for b in &owner.buses {
        let id = b.component.to_string();
        put(format!("dc_bus/{id}/vm"), "busdc", &id, "vm");
    }
    for b in owner.branches.iter().filter(|b| b.in_service) {
        let id = b.component.to_string();
        for field in ["pf", "pt"] {
            put(format!("dc_branch/{id}/{field}"), "branchdc", &id, field);
        }
    }
    for s in owner.stations.iter().filter(|s| s.in_service) {
        let id = s.component.to_string();
        for field in [
            "pgrid", "qgrid", "ptf_to", "qtf_to", "ppr_fr", "qpr_fr", "pconv", "qconv", "pdc",
            "iconv", "vmfilt", "vafilt", "vmconv", "vaconv",
        ] {
            put(format!("station/{id}/{field}"), "convdc", &id, field);
        }
    }
    x
}

/// Two deterministic off-feasible points exercise nonzero internal angle and
/// voltage differences. Keep voltage/current positive, without clipping to any
/// operating bound; these points test calculus, not solver feasibility.
#[allow(dead_code)] // Shared by the equation-only example as well.
pub fn derivative_points(point: &BTreeMap<String, f64>) -> Vec<(String, BTreeMap<String, f64>)> {
    let mut points = vec![("reference".into(), point.clone())];
    for (name, sign) in [("perturbed_plus", 1.0), ("perturbed_minus", -1.0)] {
        let changed = point
            .iter()
            .enumerate()
            .map(|(i, (name, &value))| {
                let delta = sign * 0.03 * ((i * 37 % 101) as f64 - 50.0) / 50.0;
                let positive = name.starts_with("vm[")
                    || name.ends_with("/vm")
                    || name.ends_with("/vmfilt")
                    || name.ends_with("/vmconv")
                    || name.ends_with("/iconv");
                (
                    name.clone(),
                    if positive {
                        value * (1.0 + delta)
                    } else {
                        value + delta
                    },
                )
            })
            .collect();
        points.push((name.into(), changed));
    }
    points
}

/// A failed reference PF is evidence of a local solver outcome, not a feasible
/// point or proof of infeasibility. OPF failure always invalidates the harness.
#[allow(dead_code)]
pub fn qualified(oracle: &Value, mode: &str) -> bool {
    let solve = &oracle["solves"][mode];
    match solve["termination_status"]
        .as_str()
        .expect("reference status")
    {
        "LOCALLY_SOLVED" | "OPTIMAL" => {
            assert_ne!(solve["qualified"], false);
            true
        }
        "LOCALLY_INFEASIBLE" => {
            assert_eq!(mode, "pf", "OPF reference must solve");
            assert_eq!(solve["qualified"], false, "failure must be explicit");
            false
        }
        status => panic!("Unqualified reference status: {status}"),
    }
}

#[allow(dead_code)]
pub fn instance(net: BalancedNetwork, map: &Value) -> powerio::AcDcOpfInstance {
    let policy = match map["_angle_reference_policy"].as_str() {
        None | Some("declared") => powerio::AcDcAngleReferencePolicy::Declared,
        Some("lowest_bus_id_per_island") => powerio::AcDcAngleReferencePolicy::LowestBusIdPerIsland,
        _ => panic!("Unknown fixture angle-reference policy"),
    };
    powerio::AcDcOpfInstance::from_network(
        net,
        powerio::AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
    )
    .unwrap()
    .with_angle_reference_policy(policy)
}
