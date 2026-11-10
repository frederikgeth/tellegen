#![cfg(feature = "acopf")]
#[path = "support/acdc_reference.rs"]
mod reference;
use std::path::Path;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

fn equations(case: &str, buses: usize, generators: usize, branches: usize, variables: usize) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc")
        .join(case);
    let (net, oracle, map) = reference::load(&dir);
    let original = serde_json::to_value(&net).unwrap();
    assert_eq!(
        (
            net.buses().len(),
            net.generators().len(),
            net.branches().len()
        ),
        (buses, generators, branches)
    );
    assert_eq!(oracle["source_sha256"], map["_source_sha256"]);
    assert!(!reference::qualified(&oracle, "pf"));
    assert!(
        oracle["solves"]["pf"]["max_primal_violation"]
            .as_f64()
            .unwrap()
            > 1e-3
    );
    assert!(
        oracle["solves"]["pf"].get("solution").is_none(),
        "failed PF must not supply a qualified point"
    );
    assert!(reference::qualified(&oracle, "opf"));
    let model = ReferenceEquations::powermodels_acdc_0101(&net, EquationMode::Opf).unwrap();
    let point = reference::point(&net, &oracle["solves"]["opf"]["solution"], &map);
    assert_eq!(point.len(), variables);
    let report = model.evaluate(&point).unwrap();
    assert!(
        report.max_violation < 1e-7,
        "{case}: {}",
        report.max_violation
    );
    assert!(
        (report.objective - oracle["solves"]["opf"]["objective"].as_f64().unwrap()).abs() < 1e-7
    );
    assert_eq!(serde_json::to_value(&net).unwrap(), original);
}

#[test]
fn case39_equations_keep_failed_reference_pf_unqualified() {
    equations("case39", 39, 10, 46, 456);
}

fn solves(case: &str, variables: usize) {
    use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfStart, PreparedAcDcOpf};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../evals/validation/acdc")
        .join(case);
    let (net, oracle, map) = reference::load(&dir);
    let intent =
        powerio::AcDcOpfInstance::from_network(net, AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1)
            .unwrap();
    let prepared = PreparedAcDcOpf::new(intent).unwrap();
    let point = reference::point(
        prepared.instance().network(),
        &oracle["solves"]["opf"]["solution"],
        &map,
    );
    let warm = AcDcOpfStart::Point {
        model_fingerprint: prepared.manifest().model_fingerprint.clone(),
        values: point,
    };
    for start in [warm, AcDcOpfStart::Declared, AcDcOpfStart::Flat] {
        let result = prepared.solve(start, None).unwrap();
        assert_eq!(result.point.len(), variables);
        assert!(result.validation.max_violation < 1e-7);
        assert!(result.solver_kkt_error < 1e-6);
        let target = oracle["solves"]["opf"]["objective"].as_f64().unwrap();
        assert!((result.validation.objective - target).abs() < 1e-7 + 1e-8 * target.abs());
        let module = powerio::PioModule::new(powerio::PioValue::from(result.solution.clone()));
        let output = powerio::serialize(
            &module,
            powerio::Destination::memory("result.json").unwrap(),
        )
        .unwrap();
        let powerio::EmittedOutput::Memory { mut artifacts } = output.into_output() else {
            panic!()
        };
        let restored = powerio::deserialize(
            powerio::Source::from_memory("result.json", artifacts.pop().unwrap().into_bytes())
                .unwrap(),
        )
        .unwrap();
        let powerio::PioValue::AcDcOpfSolution(solution) = restored.value() else {
            panic!()
        };
        assert_eq!(solution.point(), result.solution.point());
        let intent = prepared
            .instance()
            .clone()
            .with_initial_point(solution.point().clone())
            .unwrap();
        let restored = PreparedAcDcOpf::new(intent).unwrap();
        assert!(
            restored
                .evaluate(
                    &restored
                        .initial_point(&AcDcOpfStart::InstancePoint)
                        .unwrap()
                )
                .unwrap()
                .max_violation
                < 1e-7
        );
    }
}

#[test]
fn case39_three_native_starts_and_physical_ir_round_trips() {
    solves("case39", 456);
}
#[test]
fn case67_refuses_missing_ac_angle_reference_without_mutating_source() {
    use tellegen::ac_dc::{AcDcOpfProfile, PreparedAcDcOpf};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc/case67");
    let (net, oracle, _) = reference::load(&dir);
    let original = serde_json::to_value(&net).unwrap();
    assert!(!reference::qualified(&oracle, "pf"));
    assert!(reference::qualified(&oracle, "opf"));
    let gauges = oracle["solves"]["opf"]["ac_angle_gauge"]
        .as_array()
        .unwrap();
    let ungrounded: Vec<_> = gauges
        .iter()
        .filter(|g| g["reference_buses"].as_array().unwrap().is_empty())
        .collect();
    assert_eq!(ungrounded.len(), 1);
    assert_eq!(ungrounded[0]["buses"], serde_json::json!([67]));
    assert_eq!(
        ungrounded[0]["shifted_variables"],
        serde_json::json!(["0_va[67]", "0_vac[9]", "0_vaf[9]"])
    );
    for probe in ungrounded[0]["probes"].as_array().unwrap() {
        assert!(probe["max_primal_violation"].as_f64().unwrap() < 1e-7);
        assert!(probe["objective_difference"].as_f64().unwrap().abs() < 1e-7);
    }
    let intent = powerio::AcDcOpfInstance::from_network(
        net.clone(),
        AcDcOpfProfile::PowerModelsAcDc0101AcpVscV1,
    )
    .unwrap();
    let error = PreparedAcDcOpf::new(intent)
        .err()
        .expect("missing AC reference must refuse");
    assert!(
        error.contains("1 connected component(s) have no reference"),
        "{error}"
    );
    assert_eq!(serde_json::to_value(&net).unwrap(), original);
}

#[test]
#[should_panic(expected = "OPF reference must solve")]
fn failed_opf_cannot_be_skipped_as_a_known_pf_failure() {
    let oracle = serde_json::json!({"solves":{"opf":{"termination_status":"LOCALLY_INFEASIBLE", "qualified":false}}});
    reference::qualified(&oracle, "opf");
}
