#!/usr/bin/env python3
"""Generate physical phasor cases and check them in pinned PowerModelsACDC."""
import argparse
import json
import os
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
out = args.output.resolve()
if out == ROOT or ROOT in out.parents:
    parser.error("use an output directory outside the checkout")
out.mkdir(parents=True, exist_ok=True)
(out / "summary.json").unlink(missing_ok=True)
# Fail if the dependency changes without revisiting this pinned qualification.
assert "552fc413647307322aff7c69688e6b77ff308ad3" in (ROOT / "Cargo.lock").read_text()
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
env = os.environ.copy()
env.setdefault("CARGO_INCREMENTAL", "0")
env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
env.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
with (out / "native.log").open("w") as log:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--offline", "--example", "acdc_physical", "--", str(out)],
                   cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
with (out / "julia.log").open("w") as log:
    subprocess.run(["julia", f"--project={HERE.parent}", "--startup-file=no",
                    str(HERE / "check.jl"), str(out)], cwd=ROOT,
                   stdout=log, stderr=subprocess.STDOUT, check=True)
records = json.loads((out / "summary.json").read_text())["records"]
print(f"{len(records)} checked; {sum(r['expected_feasible'] for r in records)} feasible; "
      f"max other violation {max(r['max_other_violation'] for r in records):.3e}")
