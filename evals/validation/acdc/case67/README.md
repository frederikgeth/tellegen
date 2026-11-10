# Case67: reference-angle qualification gap

```sh
python3 evals/validation/acdc/run.py /tmp/tellegen-case67 --case case67
```

This command currently **exits nonzero at native preparation** and writes
`failure.json`; it must not produce a success summary. The original reference
PF/OPF solves and angle-shift probes run first. Case67 is not native-qualified.

`case67.m` is byte-exact [PGLib-HVDC at f928f95c](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case67.m):
30,996 bytes, 329 lines, SHA-256
`1f7637a093a5c9968f5571b323875a57f60f319a665600e0b5e11d9463c9cbf1`.
The source retains attribution and is CC-BY-4.0 under [the upstream license](../LICENSE).
There are 67 AC buses, 20 generators, 102 AC branches, nine DC buses,
11 DC branches and nine VSCs. Unlike the other fixtures, it already uses
canonical `busdc/branchdc/convdc` names rather than PGLib aliases.
The checked source-identity map remains explicit and does not use solver indices.

## Evidence, 2026-11-10

PowerIO stays pinned to `98d5c88f`, PowerModelsACDC to `9ada4082` (0.10.1).
The independent OPF solves with objective 122253.02388749263 and maximum
violation 6.822e-13. Reference PF returns `LOCALLY_INFEASIBLE`, violation
0.5991975989165781; it supplies no accepted native test point.

The AC graph has two components: buses 1–66 with reference bus 1, and singleton
bus 67 with its generator and converter 9. Bus 67 is declared PV and has no
AC branch. DC connectivity does not fix its AC phase. PowerModels collects
only declared reference buses and constrains their angles; it does not add an
angle reference for bus 67. The PowerIO canonical preparation instead requires
reference coverage in each AC component and rejects this original input.

The independent Julia probe shifts `va[67]`, `vaf[9]` and `vac[9]` together by
both -0.05 and +0.05 radians. The complete reference model, including bounds,
remains feasible at 6.822e-13, with exactly zero objective change in both probes.
`reference.json` records the component membership, shifted coordinates and
results. It omits both solved points because no native acceptance is claimed.
This confirms a free common phase at this solution, not a PowerModelsACDC bug.
No source bus type, reference constraint, or dependency pin was changed.

`failure.json` records the native gap. A regression test verifies the missing
reference rejection, preserved source, known PF status and recorded phase probes.
Native derivatives, three-start OPF and cross-feasibility remain unqualified.

## Next implementation gate

Define an explicit canonical AC/DC numerical-angle-reference policy, separate
from source bus roles and PF voltage/generation controls. Keep ordinary AC-only
reference-coverage guards. The preparation should record every added numerical
anchor and its deterministic component/identity selection without editing source
bus kinds. Include that policy in model manifests and fingerprints.

For qualification, align independent reference starts by shifting each affected
component's bus and converter internal angles together; record the shifts and
check all internal-angle bounds. Do not silently rotate only bus angles or
claim that arbitrary rotations preserve finite angle bounds. Then run the
original case67 equations, derivatives, three OPF starts, independent acceptance,
corruption controls and physical IR reloads, followed by case5/24/39 regressions.
This is an extension to canonical model preparation, not a reader repair.
