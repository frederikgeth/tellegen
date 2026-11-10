#!/usr/bin/env python3
"""Qualify native coupled PF controls against pinned, allocation-completed PMACDC."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
PIN="0ed1174cfb66975fedd0ad13af1eca5ee878354b"
p=argparse.ArgumentParser(description=__doc__)
p.add_argument("output",type=Path)
p.add_argument("--suite",choices=["case5","case24","shared","case39","case67"],default="case5")
args=p.parse_args()
suite=args.suite
count=11 if suite=="case24" else 8
out=args.output.resolve()
if out==ROOT or ROOT in out.parents:
    p.error("use an output directory outside the checkout")
out.mkdir(parents=True,exist_ok=True)
for name in ("native.json","reference.json","native-cross-check.json","qualification.json","refusals.json","control-only-native.json","control-only-reference.json"):
    (out/name).unlink(missing_ok=True)
assert PIN in (ROOT/"Cargo.lock").read_text()
subprocess.run(["python3",".github/scripts/powerio-pin.py"],cwd=ROOT,check=True)
env=os.environ.copy()
env["TELLEGEN_ACDC_PF_SUITE"]=suite
env["TELLEGEN_ACDC_CASE"]="case24" if suite=="shared" else suite
for k in ("CARGO_INCREMENTAL","CARGO_PROFILE_DEV_DEBUG","CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(k,"0")
cmd=["cargo","run","-p","tellegen","--features","acopf","--locked","--offline","--example","acdc_pf","--",str(out)]
for stage,command in [("native",cmd),("julia",["julia",f"--project={HERE.parent}","--startup-file=no",str(HERE/"check.jl"),str(out)]),("native-cross-check",cmd+["verify"])]:
    with (out/f"{stage}.log").open("w") as log:
        subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
native=json.loads((out/"native.json").read_text())
reference=json.loads((out/"reference.json").read_text())
cross=json.loads((out/"native-cross-check.json").read_text())
assert len(native)==len(reference["cases"])==count
reference_successes=sum("point" in s for c in reference["cases"] for s in c["solves"])
assert cross["checks"]==reference_successes
assert reference_successes==3*count or suite in ("case39","case67")
assert reference["powerio_revision"]==PIN
compact={k:v for k,v in reference.items() if k!="cases"}
compact["counts"]={"cases":count,"native_solves":3*count,"reference_solves":reference_successes,"reference_failed_trials":3*count-reference_successes,"bidirectional_cross_checks":3*count+reference_successes,"corruption_controls":sum(s.get("corruption_controls",0) for c in reference["cases"] for s in c["solves"])}
compact["reference_report_sha256"]=hashlib.sha256((out/"reference.json").read_bytes()).hexdigest()
compact["native_cross_check"]=cross
profiles={n["preparation"]["profile"] for n in native}
assert len(profiles)==1
compact["profile"]=profiles.pop()
source_suite="case24" if suite=="shared" else suite
spec=json.loads((HERE.parent/"cases.json").read_text())[source_suite]
source=HERE.parent/("" if source_suite=="case5" else source_suite)/spec["source_file"]
assert hashlib.sha256(source.read_bytes()).hexdigest()==spec["source_sha256"]
compact["source_case"]=spec
compact["harness_sha256"]={str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [HERE/"check.jl",HERE/"mapping.jl",HERE/"shared-voltage.jl",HERE/"case39-probes.jl",HERE/"run.py",ROOT/"crates/tellegen/examples/acdc_pf.rs",ROOT/"crates/tellegen/tests/support"/{"case5":"acdc_pf.rs","case24":"acdc_pf_case24.rs","shared":"acdc_pf_shared.rs","case39":"acdc_pf_case39.rs","case67":"acdc_pf_droop.rs"}[suite]]}
if suite=="case24":
    compact["source_qualification"]="unmodified_source_refuses_generator_converter_Vac_overlap_at_215; all_positive_cases_explicitly_switch_converter_6_to_retained_PCC_Q"
    compact["refusals"]=json.loads((out/"refusals.json").read_text())
    assert len(compact["refusals"])==11
    compact["counts"]["unmodified_source_point_checks"]=3
    catalog=json.loads((HERE.parent/"cases.json").read_text())
    compact["larger_source_specs"]={name:catalog[name] for name in ("case39","case67")}
    for name,entry in compact["larger_source_specs"].items():
        assert hashlib.sha256((HERE.parent/name/entry["source_file"]).read_bytes()).hexdigest()==entry["source_sha256"]
if suite=="case39":
    compact["source_qualification"]="explicit_DC_voltage_controller_and_fixed_OPF_Pg_variant; control_only_trials_separately_reported; source_file_unchanged"
    compact["refusals"]=json.loads((out/"refusals.json").read_text())
    assert len(compact["refusals"])==5
    compact["counts"]["control_only_native_trials"]=6
    compact["counts"]["control_only_reference_trials"]=6
    compact["dispatch_source"]={"kind":"fixed_generator_Pg_only_from_previously_qualified_OPF","reference_sha256":hashlib.sha256((HERE.parent/"case39/reference.json").read_bytes()).hexdigest(),"identity_map_sha256":hashlib.sha256((HERE.parent/"case39/identity-map.json").read_bytes()).hexdigest()}
    compact["harness_sha256"]["evals/validation/acdc/case39/reference.json"]=compact["dispatch_source"]["reference_sha256"]
    compact["harness_sha256"]["evals/validation/acdc/case39/identity-map.json"]=compact["dispatch_source"]["identity_map_sha256"]
    compact["control_only_native_sha256"]=hashlib.sha256((out/"control-only-native.json").read_bytes()).hexdigest()
    compact["control_only_reference_sha256"]=hashlib.sha256((out/"control-only-reference.json").read_bytes()).hexdigest()
    probes=json.loads((out/"control-only-native.json").read_text())
    before=probes[0]["trials"][0]["initial_point"]
    after=native[0]["solves"][0]["result"]["initial_point"]
    compact["dispatch_source"]["generator_pg_pu"]={k:{"source":v,"selected":after[k]} for k,v in before.items() if k.startswith("pg[")}
    assert len(compact["dispatch_source"]["generator_pg_pu"])==10
if suite=="case67":
    compact["refusals"]=json.loads((out/"refusals.json").read_text())
    assert len(compact["refusals"])==11
    compact["source_qualification"]="explicit_bus_67_PV_to_reference; source_generator_dispatch_and_converter_controls_retained; opt_in_DC_power_droop_and_shared_Q_injection_increments"
    contract_fixture=ROOT/"crates/tellegen/tests/support/acdc_pf_case67_contract.rs"
    compact["harness_sha256"][str(contract_fixture.relative_to(ROOT))]=hashlib.sha256(contract_fixture.read_bytes()).hexdigest()
    compact["saturation_follow_up"]="planned_separate_policy; current_profile_reports_limits_without_saturation_or_control_switching"
if suite=="shared":
    compact["source_qualification"]="original_case24_controls_retained; explicit_shared_voltage_equal_injection_increment_profile"
    compact["additional_source_case"]=json.loads((HERE.parent/"cases.json").read_text())["case5"]
    entry=compact["additional_source_case"]
    assert hashlib.sha256((HERE.parent/entry["source_file"]).read_bytes()).hexdigest()==entry["source_sha256"]
    compact["counts"]["unmodified_source_point_checks"]=3
compact["scope"]="source_pu_Cinv_PF_with_explicit_generator_allocation; limits_reported_after_solve; no_physical_fixed_mode_PF_or_automatic_control_switching"
compact["acceptance"]={"native_residual_tolerance":1e-10,"cross_constraint_tolerance":1e-7,"electrical_coordinate_tolerance":1e-7,"angle_comparison":"phase_modulo_2pi; raw_points_retained"}
compact["cases"]={}
for n,r in zip(native,reference["cases"]):
    assert n["case"]==r["case"] and len(n["solves"])==len(r["solves"])==3
    ns=[s["result"] for s in n["solves"]];rs=[s for s in r["solves"] if "point" in s]
    failed=[{k:v for k,v in s.items() if k!="failed_reference_iterate"} for s in r["solves"] if "point" not in s]
    compact["cases"][n["name"]]={
        "reference_failed_trials":failed,"reference_successful_starts":[s["start"] for s in rs],
        "source_sha256":r["source_sha256"],"model_fingerprint":n["preparation"]["model_fingerprint"],
        "variables":len(n["preparation"]["variables"]),"equalities":len(n["preparation"]["equalities"]),
        "controls":n["preparation"]["controls"],"sharing_equalities":r["sharing_equalities"],
        "duplicate_voltage_rows_restored":[s["duplicate_voltage_rows_restored"] for s in rs],
        "native_iterations":[s["iterations"] for s in ns],
        "native_current_retractions":[s["current_retractions"] for s in ns],
        "max_native_violation":max(s["validation"]["max_violation"] for s in ns),
        "max_reference_violation":max(s["reference_violation"] for s in rs),
        "max_unmodified_source_violation":max((s["native_in_unmodified_source_violation"] for s in rs if s["native_in_unmodified_source_violation"] is not None),default=None),
        "max_native_in_reference_violation":max(s["native_in_reference_violation"] for s in r["solves"]),
        "max_electrical_difference":max(s["max_electrical_difference"] for s in rs),
        "max_raw_coordinate_difference":max(s["max_raw_coordinate_difference"] for s in rs),
        "phase_windings_by_start":{s["start"]:s["phase_windings_relative_to_native"] for s in rs},
        "representative_point":ns[0]["point"],"representative_limit_violations":ns[0]["limit_violations"],
        "reference_only_variables":rs[0]["reference_only_variables"],
    }
# Tight ratings must report violations without changing the electrical solution.
baseline={"case5":"source","case24":"declared_q_at_215","shared":"mixed_multiple","case39":"opf_pg_vdc_converter_1","case67":"reference_67_source_dispatch"}[suite]
for k,v in compact["cases"][baseline]["representative_point"].items():
    assert abs(v-compact["cases"]["tight_ratings"]["representative_point"][k])<1e-9
assert any(e["name"].endswith("/iconv") for e in compact["cases"]["tight_ratings"]["representative_limit_violations"])
if suite!="case5":
    # Full points and control manifests remain in the hashed external reports.
    # Keep the checked-in evidence small enough to review.
    for n in native:
        c=compact["cases"][n["name"]]
        c["angle_references"]=n["preparation"]["angle_references"]
        c["bus_type_policy"]=n["preparation"]["bus_type_policy"]
        c["control_count"]=len(c.pop("controls"))
        c["active_coordinates_by_equipment"]={k:len(v) for k,v in n["ids"].items()}
        c["representative_point_sha256"]=hashlib.sha256(json.dumps(c.pop("representative_point"),sort_keys=True).encode()).hexdigest()
        violations=c.pop("representative_limit_violations")
        c["limit_violation_count"]=len(violations)
        c["max_limit_violation"]=max((e["violation"] for e in violations),default=0.)
        c["reference_only_variable_count"]=len(c.pop("reference_only_variables"))
(out/"qualification.json").write_text(json.dumps(compact,indent=2,sort_keys=True)+"\n")
print(f"{count} PF cases; {3*count+reference_successes} successful solves/checks; {3*count-reference_successes} failed reference trials; {compact['counts']['corruption_controls']} corruption controls pass")
