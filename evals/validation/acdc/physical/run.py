#!/usr/bin/env python3
"""Generate physical phasor cases and check them in pinned PowerModelsACDC."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
parser.add_argument("--profile", choices=["compatibility", "declared-current"], default="compatibility")
parser.add_argument("--suite", choices=["physical", "bypass"], default="physical")
args = parser.parse_args()
if args.suite == "bypass" and args.profile != "compatibility":
    parser.error("bypass checks both native profiles against unmodified Julia; omit --profile")
out = args.output.resolve()
if out == ROOT or ROOT in out.parents:
    parser.error("use an output directory outside the checkout")
out.mkdir(parents=True, exist_ok=True)
for stale in ("summary.json", "qualification.json"):
    (out / stale).unlink(missing_ok=True)
# Fail if the dependency changes without revisiting this pinned qualification.
assert "896fef026feceb4cc2111a4914100fe313986386" in (ROOT / "Cargo.lock").read_text()
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
env = os.environ.copy()
env.setdefault("CARGO_INCREMENTAL", "0")
env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
env.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
with (out / "native.log").open("w") as log:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--offline", "--example", f"acdc_{args.suite}", "--", str(out), args.profile],
                   cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
with (out / "julia.log").open("w") as log:
    subprocess.run(["julia", f"--project={HERE.parent}", "--startup-file=no",
                    str(HERE / "check.jl"), str(out)], cwd=ROOT,
                   stdout=log, stderr=subprocess.STDOUT, check=True)
summary_bytes = (out / "summary.json").read_bytes()
summary = json.loads(summary_bytes)
records = summary["records"]
if args.suite == "bypass":
    groups = {}
    for r in records:
        flags = "-".join(f"{key[0]}{int(r['elements'][key])}" for key in ("transformer", "reactor", "filter"))
        groups.setdefault(f"{r['kind']}/{flags}", []).append(r)
    compact = {key: value for key, value in summary.items() if key != "records"}
    compact["complete_report_sha256"] = hashlib.sha256(summary_bytes).hexdigest()
    compact["native_profiles_checked"] = ["power_models_ac_dc0101_acp_vsc_v1", "declared_current_acp_vsc_v1"]
    compact["groups"] = {}
    for name, rows in sorted(groups.items()):
        compact["groups"][name] = {
            "cases": len(rows), "expected_feasible": sum(r["expected_feasible"] for r in rows),
            "power_bases_mva": sorted({r["bases"]["mva"] for r in rows}),
            "directions": sorted({r["direction"] for r in rows}), "poles": sorted({r["poles"] for r in rows}),
            "voltage_bases_kv": sorted({(r["bases"]["pcc_kv"], r["bases"]["converter_kv"], r["bases"]["dc_kv"], r["dc_to_base_kv"]) for r in rows}),
            "max_other_violation": max(r["max_other_violation"] for r in rows),
            "max_objective_difference": max(abs(r["objective_difference"]) for r in rows),
            "reference_violation_range": [min(r["max_reference_violation"] for r in rows), max(r["max_reference_violation"] for r in rows)],
            "min_current_corruption_violation": min(r["current_equation_corruption_violation"] for r in rows),
            "representative": rows[0],
        }
    (out / "qualification.json").write_text(json.dumps(compact, indent=2, sort_keys=True) + "\n")
print(f"{len(records)} checked; {sum(r['expected_feasible'] for r in records)} feasible; "
      f"max other violation {max(r['max_other_violation'] for r in records):.3e}")
