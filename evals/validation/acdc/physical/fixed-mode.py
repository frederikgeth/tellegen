#!/usr/bin/env python3
"""Independent local OPF/base-conversion qualification for fixed valve-loss modes on three-terminal physical DC meshes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PIN = "70025e87d918be195362ca63a48c9a68d2dfb702"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
out = args.output.resolve()
if out == ROOT or ROOT in out.parents:
    parser.error("use an output directory outside the checkout")
out.mkdir(parents=True, exist_ok=True)
for name in ("native.json", "reference.json", "native-cross-check.json", "native-base-check.json", "qualification.json"):
    (out / name).unlink(missing_ok=True)
assert PIN in (ROOT / "Cargo.lock").read_text()
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
env = os.environ.copy()
for key in ("CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(key, "0")
cmd = ["cargo", "run", "-p", "tellegen", "--features", "acopf", "--locked", "--offline",
       "--example", "acdc_fixed_mode", "--", str(out)]
for stage, command in [
    ("native", cmd),
    ("julia", ["julia", f"--project={HERE.parent}", "--startup-file=no", str(HERE / "fixed-mode.jl"), str(out)]),
    ("native-cross-check", cmd + ["verify"]),
]:
    with (out / f"{stage}.log").open("w") as log:
        subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)

native = json.loads((out / "native.json").read_text())
reference = json.loads((out / "reference.json").read_text())
cross = json.loads((out / "native-cross-check.json").read_text())
assert reference["powerio_revision"] == PIN
assert len(native) == len(reference["records"]) == 96
assert cross["checks"] == 768 and reference["cross_checks"] == 1536
compact = {k: v for k, v in reference.items() if k != "records"}
compact["native_solver_options"] = sorted({s["solver_options"] for r in native for s in r["solves"]})
assert len(compact["native_solver_options"]) == 1
compact["reference_report_sha256"] = hashlib.sha256((out / "reference.json").read_bytes()).hexdigest()
compact["native_cross_check"] = cross
compact["native_base_check"] = json.loads((out / "native-base-check.json").read_text())
assert compact["native_base_check"]["checks"] == 768
compact["acceptance"] = {"max_constraint_violation": 1e-7, "objective_absolute_tolerance": 1e-6,
    "objective_relative_tolerance": 1e-8, "max_phase_domain_physical_residual": 1e-6,
    "scope": "local_optima_cross_feasibility; no_global_or_primal_uniqueness_claim"}
compact["counts"] = {"physical_networks": 24, "base_representations": 96,
    "native_solves": 192, "reference_solves": 192, "reference_corruption_controls": 576}
compact["base_configurations"] = [r["bases"] for r in native[:4]]
compact["groups"] = {}
compact["max_physical_difference_by_quantity"] = {}
for group in sorted({r["group"] for r in native}):
    rows = [r for r in native if r["group"] == group]
    refs = [r for r in reference["records"] if r["group"] == group]
    ns = [s for r in rows for s in r["solves"]]
    rs = [s for r in refs for s in r["solves"]]
    assert len(rows) == len(refs) == 4 and len(ns) == len(rs) == 8
    objectives = [s["objective"] for s in ns + rs]
    points = [s["physical"] for s in ns + rs]
    physical = points[0]
    for k in physical:
        compact["max_physical_difference_by_quantity"][k] = max(
            compact["max_physical_difference_by_quantity"].get(k, 0.0),
            max(abs(p[k] - physical[k]) for p in points))
    compact["groups"][group] = {
        "networks": 4, "native_solves": 8, "reference_solves": 8,
        "objective_range": [min(objectives), max(objectives)],
        "max_native_reference_objective_difference": max(abs(s["native_objective_difference"]) for s in rs),
        "max_native_violation": max(s["max_violation"] for s in ns),
        "max_reference_violation": max(s["max_reference_violation"] for s in rs),
        "max_native_kkt": max(s["kkt"] for s in ns),
        "max_native_physical_residual": max(s["physical_residual"] for s in ns),
        "native_iterations_range": [min(s["iterations"] for s in ns), max(s["iterations"] for s in ns)],
        "representative_physical_dispatch": {k: v for k, v in physical.items()
            if k.startswith(("pg[", "dc_bus/", "dc_branch/")) or k.endswith(("/iconv", "/pgrid"))},
        "declared_loss_modes": rows[0]["loss_modes"],
        "loss_coefficients_ohm": rows[0]["coefficients"],
        "representative_binding_variable_bounds": [k for k, (lo, hi) in rows[0]["physical_bounds"].items()
            if (lo is not None and abs(physical[k] - lo) < 1e-5)
            or (hi is not None and abs(physical[k] - hi) < 1e-5)],
        "source_hashes": {r["file"]: r["source_sha256"] for r in refs},
    }
assert len(compact["groups"]) == 24
for group, row in compact["groups"].items():
    reference_group = group.rsplit("regime", 1)[0] + "regime0"
    reference_cost = compact["groups"][reference_group]["objective_range"][0]
    row["cost_increase_over_unconstrained"] = row["objective_range"][0] - reference_cost
    if group != reference_group:
        assert row["cost_increase_over_unconstrained"] > 1.0
(out / "qualification.json").write_text(json.dumps(compact, indent=2, sort_keys=True) + "\n")
print(f"96 fixed-mode representations; 384 local solves; 2304 cross-formulation/base checks pass")
