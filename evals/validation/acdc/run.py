#!/usr/bin/env python3
"""Re-solve the original case independently, then evaluate native equations."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
CASE_SHA = "759606ec335f8706ee8631ff4833c50804dd916afb5b5e682ace4f4f886fd7f3"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("output", type=Path)
args = parser.parse_args()
out = args.output.resolve()
if out == HERE:
    parser.error("use a separate output directory; inspect results before replacing fixtures")
out.mkdir(parents=True, exist_ok=True)
assert hashlib.sha256((HERE / "case5_3_he.m").read_bytes()).hexdigest() == CASE_SHA
subprocess.run(["python3", ".github/scripts/powerio-pin.py"], cwd=ROOT, check=True)
for name in ("case5_3_he.m", "identity-map.json"):
    shutil.copyfile(HERE / name, out / name)
with (out / "julia.log").open("w") as log:
    subprocess.run(["julia", f"--project={HERE}", "--startup-file=no",
                    str(HERE / "generate.jl"), str(out / "reference.json")],
                   cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
with (out / "native-report.json").open("w") as report:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--example", "acdc_equations", "--", str(out)],
                   cwd=ROOT, stdout=report, check=True)
with (out / "derivative-report.json").open("w") as report:
    subprocess.run(["cargo", "run", "-p", "tellegen", "--features", "acopf",
                    "--locked", "--example", "acdc_derivatives", "--", str(out)],
                   cwd=ROOT, stdout=report, check=True)
reports = json.loads((out / "native-report.json").read_text())
summary = {mode: {"equations": len(r["equations"]), "bounds": len(r["bounds"]),
                  "max_violation": r["max_violation"], "objective": r["objective"]}
           for mode, r in reports.items()}
(out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
