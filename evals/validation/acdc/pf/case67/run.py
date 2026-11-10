#!/usr/bin/env python3
"""Audit the case67 PF reference/dispatch and DC-power-droop contract."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('output', type=Path)
a = p.parse_args()
out = a.output.resolve()
if out == ROOT or ROOT in out.parents:
    p.error('use an output directory outside the checkout')
out.mkdir(parents=True, exist_ok=True)
for name in ('inputs.json', 'reference.json', 'native-checks.json', 'qualification.json'):
    (out / name).unlink(missing_ok=True)
env = os.environ.copy()
for k in ('CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
    env.setdefault(k, '0')
subprocess.run(['python3', '.github/scripts/powerio-pin.py'], cwd=ROOT, check=True)
cmd = ['cargo', 'run', '-p', 'tellegen', '--features', 'acopf', '--locked', '--offline', '--example', 'acdc_pf_case67_contract', '--', str(out)]
for stage, command in [('export', cmd), ('reference', ['julia', f'--project={HERE.parent.parent}', '--startup-file=no', str(HERE / 'reference.jl'), str(out)]), ('native-checks', cmd + ['verify'])]:
    with (out / f'{stage}.log').open('w') as log:
        subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
r = json.loads((out / 'reference.json').read_text())
n = json.loads((out / 'native-checks.json').read_text())
assert len(r['cases']) == 8
successes = sum('native_point' in s for c in r['cases'] for s in c['solves'])
assert len(n) == successes and successes >= 16
q = {k: v for k, v in r.items() if k != 'cases'}
q['counts'] = {'reference_trials': 24, 'reference_solves': successes, 'reference_failed_trials': 24-successes, 'native_pf_solves': 0, 'native_electrical_and_control_checks': successes, 'reference_negative_controls': sum(s.get('negative_controls', 0) for c in r['cases'] for s in c['solves']), 'native_negative_controls': sum(s['negative_controls'] for s in n), 'droop_scalar_witnesses': sum(s.get('droop_scalar_witnesses', 0) for c in r['cases'] for s in c['solves'])}
q['native_checks'] = n
q['cases'] = []
points = {}
for c in r['cases']:
    compact = {k: v for k, v in c.items() if k != 'solves'}
    compact['solves'] = []
    baseline = next(s['native_point'] for s in c['solves'] if 'native_point' in s)
    for s in c['solves']:
        if 'native_point' not in s:
            compact['solves'].append({k: v for k, v in s.items() if k != 'failed_iterate'})
            continue
        point = s['native_point']
        compact['solves'].append({k: v for k, v in s.items() if k != 'native_point'})
        compact['solves'][-1]['point_sha256'] = hashlib.sha256(json.dumps(point, sort_keys=True).encode()).hexdigest()
        # Agreement is checked electrically, respecting phase periodicity.
        delta = [abs(math.atan2(math.sin(v - baseline[k]), math.cos(v - baseline[k]))) if k.startswith('va[') or k.endswith(('/vafilt', '/vaconv')) else abs(v - baseline[k]) for k, v in point.items()]
        compact['solves'][-1]['max_electrical_difference_between_starts'] = max(delta)
        assert max(delta) < 1e-7
    points[c['name']] = baseline
    q['cases'].append(compact)
assert max(abs(v - points['tight_ratings'][k]) for k, v in points['reference_67_source_dispatch'].items()) < 1e-7
q['harness_sha256'] = {str(f.relative_to(ROOT)): hashlib.sha256(f.read_bytes()).hexdigest() for f in [HERE / 'run.py', HERE / 'reference.jl', HERE.parent / 'mapping.jl', HERE.parent / 'shared-voltage.jl', HERE.parent.parent / 'physical/reference-numerics.jl', HERE.parent.parent / 'reference-model.jl', HERE.parent.parent / 'case67/identity-map.json', ROOT / 'crates/tellegen/examples/acdc_pf_case67_contract.rs', ROOT / 'crates/tellegen/tests/support/acdc_pf_case67_contract.rs']}
q['report_sha256'] = {name: hashlib.sha256((out / name).read_bytes()).hexdigest() for name in ('inputs.json', 'reference.json', 'native-checks.json')}
(out / 'qualification.json').write_text(json.dumps(q, indent=2, sort_keys=True) + '\n')
print(f'{successes} reference PF solves and native electrical/control checks pass; {24-successes} failed reference trials retained; native droop PF remains unimplemented')
