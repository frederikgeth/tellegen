#!/usr/bin/env python3
"""Physical-coordinate PF against explicit SI PMACDC substitutions and a circuit oracle."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PIN = "3e41473984585c0b38560f47ba3e0c2d930b029b"
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("output", type=Path)
args = p.parse_args()
out = args.output.resolve()
if out == ROOT or ROOT in out.parents:
    p.error("use an output directory outside the checkout")
out.mkdir(parents=True, exist_ok=True)
for name in ("native.json", "reference.json", "native-cross-check.json", "qualification.json"):
    (out / name).unlink(missing_ok=True)
assert PIN in (ROOT / "Cargo.lock").read_text()
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
env = os.environ.copy()
for k in ("CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(k, "0")
cmd = ["cargo", "run", "-p", "tellegen", "--features", "acopf", "--locked", "--offline", "--example", "acdc_pf_physical", "--", str(out)]
for stage, command in [("native", cmd), ("julia", ["julia", f"--project={HERE.parent}", "--startup-file=no", str(HERE / "physical.jl"), str(out)]), ("native-cross-check", cmd + ["verify"])]:
    with (out / f"{stage}.log").open("w") as log:
        subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
native = json.loads((out / "native.json").read_text())
reference = json.loads((out / "reference.json").read_text())
cross = json.loads((out / "native-cross-check.json").read_text())
assert len(native) == len(reference["records"]) == 256
assert all(r["powerio_revision"] == PIN for r in native)
compact = {"powerio_revision": PIN, "reference_revision": "9ada4082ac087aed649f48fb5f977680b0fe06c7", "profile": "physical_coordinates_dc_droop_shared_voltage_equal_increment_v1", "reference_scope": "PMACDC_0.10.1_with_explicit_physical_DC_line_transformer_bypass_and_droop_substitutions; local_roots_only", "counts": {"physical_cases": 64, "representations": 256, "native_solves": 512, "reference_solves": 512, "native_equivalent_base_checks": 2048, "reference_equivalent_base_checks": reference["cross_checks"], "reference_to_native_checks": cross["checks"], "reference_corruptions": sum(s["negative_controls"] for r in reference["records"] for s in r["solves"])}, "native_cross_check": cross, "max_reference_cross_violation": reference["max_cross_violation"], "base_configurations": [r["bases"] for r in native[:4]], "groups": {}}
compact["report_sha256"] = {name: hashlib.sha256((out / name).read_bytes()).hexdigest() for name in ("native.json", "reference.json", "native-cross-check.json")}
compact["harness_sha256"] = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in [HERE / "physical.py", HERE / "physical.jl", HERE.parent / "physical/reference-physical.jl", HERE.parent / "physical/reference-numerics.jl", ROOT / "crates/tellegen/examples/acdc_pf_physical.rs", ROOT / "crates/tellegen/tests/support/acdc_pf_physical.rs", ROOT / "crates/tellegen/tests/support/acdc_physical.rs", ROOT / "crates/tellegen/tests/support/acdc_optimized.rs"]}
for group in sorted({r["group"] for r in native}):
    rows = [r for r in native if r["group"] == group]
    refs = [r for r in reference["records"] if r["group"] == group]
    ns = [s for r in rows for s in r["solves"]]
    rs = [s for r in refs for s in r["solves"]]
    assert len(rows) == len(refs) == 4 and len(ns) == len(rs) == 8
    physical = [s["physical"] for s in ns + rs]
    delta = max(abs(point[k] - physical[0][k]) for point in physical for k in point)
    assert delta < 1e-6
    compact["groups"][group] = {"max_native_residual": max(s["max_violation"] for s in ns), "max_reference_residual": max(s["max_reference_violation"] for s in rs), "max_physical_circuit_residual": max(s["physical_residual"] for s in ns), "max_physical_coordinate_difference": delta, "native_iterations_range": [min(s["iterations"] for s in ns), max(s["iterations"] for s in ns)], "source_hashes": {r["file"]: r["source_sha256"] for r in refs}}
assert len(compact["groups"]) == 64
(out / "qualification.json").write_text(json.dumps(compact, indent=2, sort_keys=True) + "\n")
print("256 representations; 1024 local PF solves; 8192 equivalent-base checks; 1536 corruptions pass")
