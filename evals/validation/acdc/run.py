#!/usr/bin/env python3
"""Re-solve the original case independently, then evaluate native equations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CASES = json.loads((HERE / "cases.json").read_text())
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
parser.add_argument("--case", choices=list(CASES), default="case5")
args = parser.parse_args()
fixture = HERE if args.case == "case5" else HERE / args.case
source_name = CASES[args.case]["source_file"]
source_sha = CASES[args.case]["source_sha256"]
os.environ["TELLEGEN_ACDC_CASE"] = args.case
out = args.output.resolve()
if out == HERE or HERE in out.parents:
    parser.error("use a separate output directory; inspect results before replacing fixtures")
out.mkdir(parents=True, exist_ok=True)
# A rerun which fails must not leave a previous success summary visible.
for stale in ("summary.json", "failure.json"):
    (out / stale).unlink(missing_ok=True)
assert hashlib.sha256((fixture / source_name).read_bytes()).hexdigest() == source_sha
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
for name in (source_name, "identity-map.json"):
    shutil.copyfile(fixture / name, out / name)
with (out / "julia.log").open("w") as log:
    subprocess.run(["julia", f"--project={HERE}", "--startup-file=no",
                    str(HERE / "generate.jl"), str(out / "reference.json")],
                   cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
with (out / "native-report.json").open("w") as report:
    equations = subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--example", "acdc_equations", "--", str(out)],
                   cwd=ROOT, stdout=report, stderr=subprocess.PIPE, text=True)
print(equations.stderr, file=sys.stderr, end="")
if equations.returncode:
    (out / "failure.json").write_text(json.dumps({
        "case": args.case, "source_sha256": source_sha,
        "stage": "native_equations", "qualified": False,
        "stderr": equations.stderr}, indent=2) + "\n")
    equations.check_returncode()
with (out / "derivative-report.json").open("w") as report:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--example", "acdc_derivatives", "--", str(out)],
                   cwd=ROOT, stdout=report, check=True)
with (out / "native-opf.json").open("w") as report:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--example", "acdc_opf", "--", str(out)],
                   cwd=ROOT, stdout=report, check=True)
with (out / "cross-check.log").open("w") as log:
    subprocess.run(["julia", f"--project={HERE}", "--startup-file=no",
                    str(HERE / "cross-check.jl"), str(out / "native-opf.json"),
                    str(out / "cross-check.json"), str(out / "reference.json")], cwd=ROOT,
                   stdout=log, stderr=subprocess.STDOUT, check=True)
reports = json.loads((out / "native-report.json").read_text())
summary = {mode: {"equations": len(r["equations"]), "bounds": len(r["bounds"]),
                  "max_violation": r["max_violation"], "objective": r["objective"]}
           for mode, r in reports.items()}
native = json.loads((out / "native-opf.json").read_text())
cross = json.loads((out / "cross-check.json").read_text())
summary["native_opf"] = {
    name: {"objective": r["validation"]["objective"], "iterations": r["iterations"],
           "native_violation": r["validation"]["max_violation"],
           "reference_violation": cross["solves"][name]["max_reference_violation"],
           "oracle_objective_difference": cross["solves"][name]["oracle_objective_difference"]}
    for name, r in native["solves"].items()
}
reference = json.loads((out / "reference.json").read_text())
derivatives = json.loads((out / "derivative-report.json").read_text())
assert all(r["passed"] for r in derivatives.values())
summary["provenance"] = {key: reference[key] for key in (
    "source_sha256", "manifest_sha256", "powermodelsacdc_commit", "julia",
    "powermodelsacdc", "powermodels", "jump", "ipopt")}
summary["provenance"]["powerio_revision"] = native["preparation"]["powerio_revision"]
summary["derivatives"] = {
    "points": len(derivatives), "checks": sum(len(r["checks"]) for r in derivatives.values()),
    "variables": next(iter(derivatives.values()))["variables"],
    "finite_difference_order": next(iter(derivatives.values()))["finite_difference_order"],
    "relative_steps": sorted({c["relative_step"] for r in derivatives.values() for c in r["checks"]}, reverse=True),
    "scaled_tolerance": next(iter(derivatives.values()))["scaled_tolerance"],
    "max_scaled_errors": {family: max(c["max_scaled_error"] for r in derivatives.values()
        for c in r["checks"] if c["name"].startswith(family))
        for family in ("objective_gradient", "constraint_jacobian", "hessian/")}}
summary["bus_type_policy"] = reports["opf"]["bus_type_policy"]
summary["reference_pf"] = {k: reference["solves"]["pf"][k] for k in (
    "termination_status", "qualified", "max_primal_violation")}
summary["negative_controls_detected"] = cross["negative_controls_detected"]
for name, result in summary["native_opf"].items():
    result["max_absolute_electrical_difference"] = cross["solves"][name]["max_absolute_electrical_difference"]
    result["native_variables"] = cross["solves"][name]["native_variables"]
    result["reference_variables"] = cross["solves"][name]["reference_variables"]
(out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
