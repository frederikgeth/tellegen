#!/usr/bin/env python3
"""Reproduce asymmetric loss branches and boundary witnesses, without an OPF claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PIN = "27aae3b3313b17916abd84cc33fe6a2dd459ec0b"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
out = args.output.resolve()
if out == ROOT or ROOT in out.parents:
    parser.error("use an output directory outside the checkout")
out.mkdir(parents=True, exist_ok=True)
for name in ("cases.json", "reference.json", "qualification.json"):
    (out / name).unlink(missing_ok=True)
assert PIN in (ROOT / "Cargo.lock").read_text()
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
env = os.environ.copy()
for key in ("CARGO_INCREMENTAL", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG"):
    env.setdefault(key, "0")
for name, cmd in [
    ("native", ["cargo", "run", "-p", "tellegen", "--features", "acopf", "--locked", "--offline", "--example", "acdc_losses", "--", str(out)]),
    ("julia", ["julia", f"--project={HERE.parent}", "--startup-file=no", str(HERE / "losses.jl"), str(out)]),
]:
    with (out / f"{name}.log").open("w") as log:
        subprocess.run(cmd, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
native = json.loads((out / "cases.json").read_text())
reference = json.loads((out / "reference.json").read_text())
assert len(native) == len(reference["records"]) == 1728
assert reference["powerio_revision"] == PIN
report = {k: v for k, v in reference.items() if k != "records"}
report["reference_report_sha256"] = hashlib.sha256((out / "reference.json").read_bytes()).hexdigest()
report["acceptance"] = {"other_constraint_tolerance": 1e-9, "signed_loss_residual_mw_tolerance": 1e-10,
                        "selected_branch_tolerance": 1e-9, "scope": "pointwise_only; zero_mode_unselected"}
report["counts"]["expected_source_loss_mismatches"] = sum(abs(r["source_loss_residual_mw"]) > 1e-10 for r in native)
assert report["counts"]["expected_source_loss_mismatches"] == 504
report["counts"]["named_mode_witnesses"] = sum(r["named_mode"] == r["case"]["selected"] for r in native)
report["counts"]["zero_mode_boundary_witnesses"] = sum(r["named_mode"] is None for r in native)
report["max_other_reference_violation"] = max(r["other_reference_violation"] for r in reference["records"])
report["max_selected_branch_violation"] = max(r["selected_branch_violation"] for r in reference["records"])
report["max_signed_residual_difference_mw"] = max(abs(a["source_loss_residual_mw"] - b["source_loss_residual_mw"]) for a, b in zip(native, reference["records"]))
report["base_configurations"] = [r["bases"] for r in native[:9]]
report["groups"] = {}
for operating in sorted({r["case"]["operating"] for r in native}):
    for coefficients in ([0.3, 1.3], [1.3, 0.3], [1.3, 1.3]):
        for selected in ("rectifier", "inverter"):
            rows = [r for r in native if (r["case"]["operating"], r["case"]["coefficients"], r["case"]["selected"]) == (operating, coefficients, selected)]
            assert len(rows) == 36
            # Representative full station; bypasses/poles/bases remain in full evidence.
            r = next(r for r in rows if r["case"]["mask"] == 7)
            report["groups"][f"{operating}/{coefficients}/{selected}"] = {
                "representations": len(rows), "named_mode": r["named_mode"],
                **{k: r[k] for k in ("current_ka", "pcc_mw", "valve_mw", "dc_withdrawal_mw", "selected_loss_mw", "source_loss_residual_mw")},
            }
assert len(report["groups"]) == 48
report["source_files_digest_sha256"] = hashlib.sha256(json.dumps({r["file"]: r["source_sha256"] for r in reference["records"]}, sort_keys=True).encode()).hexdigest()
(out / "qualification.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print("1728 physical branch witnesses; 504 expected source-loss mismatches; 1728 corruption controls pass")
