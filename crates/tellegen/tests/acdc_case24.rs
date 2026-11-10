#![cfg(feature = "acopf")]
#[path = "support/acdc_reference.rs"]
mod reference;
use std::path::Path;
use tellegen::ac_dc::{EquationMode, ReferenceEquations};

#[test]
fn case24_reference_equations_keep_source_and_record_pq_to_pv_repair() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc/case24");
    let (net, oracle, map) = reference::load(&dir);
    let original = serde_json::to_value(&net).unwrap();
    assert_eq!(
        (
            net.buses().len(),
            net.generators().len(),
            net.branches().len()
        ),
        (50, 65, 77)
    );
    assert_eq!(oracle["source_sha256"], map["_source_sha256"]);
    assert_eq!(oracle["effective_bus_types"]["123"], 2);
    assert_eq!(
        net.buses().iter().find(|b| b.id.0 == 123).unwrap().kind,
        powerio::BusType::Pq
    );
    for (label, mode) in [("pf", EquationMode::Pf), ("opf", EquationMode::Opf)] {
        let model = ReferenceEquations::powermodels_acdc_0101(&net, mode).unwrap();
        let point = reference::point(&net, &oracle["solves"][label]["solution"], &map);
        let report = model.evaluate(&point).unwrap();
        assert!(
            report.max_violation < 1e-7,
            "{label}: {}",
            report.max_violation
        );
        assert_eq!(report.bus_type_policy.len(), 1);
        assert_eq!(report.bus_type_policy[0].bus_id, 123);
        assert_eq!(report.bus_type_policy[0].declared, powerio::BusType::Pq);
        assert_eq!(report.bus_type_policy[0].effective, powerio::BusType::Pv);
        if mode == EquationMode::Pf {
            assert!(report.equations.iter().any(|r| r.name == "pf/vm[123]"));
            assert!(!report
                .equations
                .iter()
                .any(|r| r.name.starts_with("pf/qg[bus-123")));
        } else {
            assert!(
                (report.objective - oracle["solves"][label]["objective"].as_f64().unwrap()).abs()
                    < 1e-7
            );
        }
    }
    assert_eq!(serde_json::to_value(&net).unwrap(), original);
}

#[test]
fn case24_native_opf_converges_from_three_starts_and_persists_physical_points() {
    use tellegen::ac_dc::{AcDcOpfProfile, AcDcOpfStart, PreparedAcDcOpf};
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evals/validation/acdc/case24");
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
        assert_eq!(result.point.len(), 657);
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
