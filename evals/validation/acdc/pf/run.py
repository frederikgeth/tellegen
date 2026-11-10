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
PIN="e5a7c8b82845e32de7427f39ff8e63768f5f8bd2"
p=argparse.ArgumentParser(description=__doc__)
p.add_argument("output",type=Path)
p.add_argument("--suite",choices=["case5","case24"],default="case5")
args=p.parse_args()
suite=args.suite
count=8 if suite=="case5" else 11
out=args.output.resolve()
if out==ROOT or ROOT in out.parents:
    p.error("use an output directory outside the checkout")
out.mkdir(parents=True,exist_ok=True)
for name in ("native.json","reference.json","native-cross-check.json","qualification.json","refusals.json"):
    (out/name).unlink(missing_ok=True)
assert PIN in (ROOT/"Cargo.lock").read_text()
subprocess.run(["python3",".github/scripts/powerio-pin.py"],cwd=ROOT,check=True)
env=os.environ.copy()
env["TELLEGEN_ACDC_PF_SUITE"]=suite
env["TELLEGEN_ACDC_CASE"]=suite
for k in ("CARGO_INCREMENTAL","CARGO_PROFILE_DEV_DEBUG","CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(k,"0")
cmd=["cargo","run","-p","tellegen","--features","acopf","--locked","--offline","--example","acdc_pf","--",str(out)]
for stage,command in [("native",cmd),("julia",["julia",f"--project={HERE.parent}","--startup-file=no",str(HERE/"check.jl"),str(out)]),("native-cross-check",cmd+["verify"])]:
    with (out/f"{stage}.log").open("w") as log:
        subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
native=json.loads((out/"native.json").read_text())
reference=json.loads((out/"reference.json").read_text())
cross=json.loads((out/"native-cross-check.json").read_text())
assert len(native)==len(reference["cases"])==count and cross["checks"]==3*count
assert reference["powerio_revision"]==PIN
compact={k:v for k,v in reference.items() if k!="cases"}
compact["counts"]={"cases":count,"native_solves":3*count,"reference_solves":3*count,"bidirectional_cross_checks":6*count,"corruption_controls":12*count}
compact["reference_report_sha256"]=hashlib.sha256((out/"reference.json").read_bytes()).hexdigest()
compact["native_cross_check"]=cross
spec=json.loads((HERE.parent/"cases.json").read_text())[suite]
source=HERE.parent/("" if suite=="case5" else suite)/spec["source_file"]
assert hashlib.sha256(source.read_bytes()).hexdigest()==spec["source_sha256"]
compact["source_case"]=spec
compact["harness_sha256"]={str(path.relative_to(ROOT)):hashlib.sha256(path.read_bytes()).hexdigest() for path in [HERE/"check.jl",HERE/"mapping.jl",HERE/"run.py",ROOT/"crates/tellegen/examples/acdc_pf.rs",ROOT/"crates/tellegen/tests/support"/("acdc_pf.rs" if suite=="case5" else "acdc_pf_case24.rs")]}
if suite=="case24":
    compact["source_qualification"]="unmodified_source_refuses_generator_converter_Vac_overlap_at_215; all_positive_cases_explicitly_switch_converter_6_to_retained_PCC_Q"
    compact["refusals"]=json.loads((out/"refusals.json").read_text())
    assert len(compact["refusals"])==11
    compact["counts"]["unmodified_source_point_checks"]=3
    catalog=json.loads((HERE.parent/"cases.json").read_text())
    compact["larger_source_specs"]={name:catalog[name] for name in ("case39","case67")}
    for name,entry in compact["larger_source_specs"].items():
        assert hashlib.sha256((HERE.parent/name/entry["source_file"]).read_bytes()).hexdigest()==entry["source_sha256"]
compact["scope"]="source_pu_Cinv_PF_with_explicit_generator_allocation; limits_reported_after_solve; no_physical_fixed_mode_PF_or_automatic_control_switching"
compact["acceptance"]={"native_residual_tolerance":1e-10,"cross_constraint_tolerance":1e-7,"electrical_coordinate_tolerance":1e-7,"angle_comparison":"phase_modulo_2pi; raw_points_retained"}
compact["cases"]={}
for n,r in zip(native,reference["cases"]):
    assert n["case"]==r["case"] and len(n["solves"])==len(r["solves"])==3
    ns=[s["result"] for s in n["solves"]];rs=r["solves"]
    compact["cases"][n["name"]]={
        "source_sha256":r["source_sha256"],"model_fingerprint":n["preparation"]["model_fingerprint"],
        "variables":len(n["preparation"]["variables"]),"equalities":len(n["preparation"]["equalities"]),
        "controls":n["preparation"]["controls"],"sharing_equalities":r["sharing_equalities"],
        "native_iterations":[s["iterations"] for s in ns],
        "native_current_retractions":[s["current_retractions"] for s in ns],
        "max_native_violation":max(s["validation"]["max_violation"] for s in ns),
        "max_reference_violation":max(s["reference_violation"] for s in rs),
        "max_unmodified_source_violation":max((s["native_in_unmodified_source_violation"] for s in rs if s["native_in_unmodified_source_violation"] is not None),default=None),
        "max_native_in_reference_violation":max(s["native_in_reference_violation"] for s in rs),
        "max_electrical_difference":max(s["max_electrical_difference"] for s in rs),
        "max_raw_coordinate_difference":max(s["max_raw_coordinate_difference"] for s in rs),
        "phase_windings_by_start":{s["start"]:s["phase_windings_relative_to_native"] for s in rs},
        "representative_point":ns[0]["point"],"representative_limit_violations":ns[0]["limit_violations"],
        "reference_only_variables":rs[0]["reference_only_variables"],
    }
# Tight ratings must report violations without changing the electrical solution.
baseline="source" if suite=="case5" else "declared_q_at_215"
for k,v in compact["cases"][baseline]["representative_point"].items():
    assert abs(v-compact["cases"]["tight_ratings"]["representative_point"][k])<1e-9
assert any(e["name"].endswith("/iconv") for e in compact["cases"]["tight_ratings"]["representative_limit_violations"])
if suite=="case24":
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
print(f"{count} PF cases; {6*count} solves; {6*count} cross-checks; {12*count} corruption controls pass")
