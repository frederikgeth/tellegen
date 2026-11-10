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
out=p.parse_args().output.resolve()
if out==ROOT or ROOT in out.parents:
    p.error("use an output directory outside the checkout")
out.mkdir(parents=True,exist_ok=True)
for name in ("native.json","reference.json","native-cross-check.json","qualification.json"):
    (out/name).unlink(missing_ok=True)
assert PIN in (ROOT/"Cargo.lock").read_text()
subprocess.run(["python3",".github/scripts/powerio-pin.py"],cwd=ROOT,check=True)
env=os.environ.copy()
for k in ("CARGO_INCREMENTAL","CARGO_PROFILE_DEV_DEBUG","CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(k,"0")
cmd=["cargo","run","-p","tellegen","--features","acopf","--locked","--offline","--example","acdc_pf","--",str(out)]
for stage,command in [("native",cmd),("julia",["julia",f"--project={HERE.parent}","--startup-file=no",str(HERE/"check.jl"),str(out)]),("native-cross-check",cmd+["verify"])]:
    with (out/f"{stage}.log").open("w") as log:
        subprocess.run(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
native=json.loads((out/"native.json").read_text())
reference=json.loads((out/"reference.json").read_text())
cross=json.loads((out/"native-cross-check.json").read_text())
assert len(native)==len(reference["cases"])==8 and cross["checks"]==24
assert reference["powerio_revision"]==PIN
compact={k:v for k,v in reference.items() if k!="cases"}
compact["counts"]={"cases":8,"native_solves":24,"reference_solves":24,"bidirectional_cross_checks":48,"corruption_controls":96}
compact["reference_report_sha256"]=hashlib.sha256((out/"reference.json").read_bytes()).hexdigest()
compact["native_cross_check"]=cross
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
        "max_native_violation":max(s["validation"]["max_violation"] for s in ns),
        "max_reference_violation":max(s["reference_violation"] for s in rs),
        "max_native_in_reference_violation":max(s["native_in_reference_violation"] for s in rs),
        "max_electrical_difference":max(s["max_electrical_difference"] for s in rs),
        "max_raw_coordinate_difference":max(s["max_raw_coordinate_difference"] for s in rs),
        "phase_windings_by_start":{s["start"]:s["phase_windings_relative_to_native"] for s in rs},
        "representative_point":ns[0]["point"],"representative_limit_violations":ns[0]["limit_violations"],
        "reference_only_variables":rs[0]["reference_only_variables"],
    }
# Tight ratings must report violations without changing the electrical solution.
for k,v in compact["cases"]["source"]["representative_point"].items():
    assert abs(v-compact["cases"]["tight_ratings"]["representative_point"][k])<1e-9
assert any(e["name"].endswith("/iconv") for e in compact["cases"]["tight_ratings"]["representative_limit_violations"])
(out/"qualification.json").write_text(json.dumps(compact,indent=2,sort_keys=True)+"\n")
print("8 PF cases; 48 solves; 48 cross-checks; 96 corruption controls pass")
