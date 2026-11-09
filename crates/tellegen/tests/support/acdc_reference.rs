use powerio::{BalancedNetwork, PioValue};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub fn load(dir: &Path) -> (BalancedNetwork, Value, Value) {
    let module =
        powerio::parse(dir.join("case5_3_he.m")).expect("parse case5 through pinned PowerIO");
    let PioValue::BalancedNetwork(net) = module.into_value() else {
        panic!("balanced case required")
    };
    let read = |name| serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap();
    (net, read("reference.json"), read("identity-map.json"))
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
    for g in net.generators() {
        let id = g.uid.as_deref().unwrap();
        for field in ["pg", "qg"] {
            put(format!("{field}[{id}]"), "gen", id, field);
        }
    }
    for b in net.branches() {
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
    for b in &owner.branches {
        let id = b.component.to_string();
        for field in ["pf", "pt"] {
            put(format!("dc_branch/{id}/{field}"), "branchdc", &id, field);
        }
    }
    for s in &owner.stations {
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
