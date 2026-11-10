# Case24 native AC/DC qualification

Run from the repository root:

```sh
CARGO_TARGET_DIR=/tmp/tellegen-acdc-target python3 evals/validation/acdc/run.py /tmp/tellegen-case24 --case case24
```

The source is `case24_7_jb.m` from [PGLib-HVDC commit
f928f95c](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case24_7_jb.m),
vendored byte-exact: 24,139 bytes, 365 lines, SHA-256
`fe8c31d586ef6853257a0ec9a018c2b92da030808e58e57b309deac1eb5ae4b4`.
The data is CC-BY-4.0 under the upstream [license](../LICENSE); source comments
retain the RTS authors' attribution and case modifications. No new solver
or runtime dependency is introduced.

The name understates the size: 50 AC buses, 65 generators, 77 AC branches,
seven DC buses and seven VSC stations. References 113, 213 and 302 anchor the
three AC islands. All seven station reactors are bypassed and filters disabled;
transformers are enabled. The fixed crosswalk in `identity-map.json` uses actual
AC bus IDs and pinned MATPOWER source-row identities for generators/branches,
not the solver's dense ordering. Parser identities were checked against source
bus/endpoints before recording the map.

`reference.json` retains independently solved PF/OPF points and provenance,
omitting bulky starts and duplicate equipment snapshots. Its 74,497 bytes / 2,648
lines are generated validation evidence, not another source case. The complete
reference, derivative, native and cross-check reports are regenerated outside
the repository. `summary.json` records their numerical acceptance results.

## Results, 2026-11-10

PowerIO remains pinned to `98d5c88f`; PowerModelsACDC to `9ada4082` (0.10.1).
Fresh reference PF/OPF solutions satisfy native equations at 4.552e-14 and
1.023e-10 maximum violation. There are 657 native variables; independent Julia
checks account for all 766 reference variables, including 109 fixed-demand and
unused VSC auxiliaries reconstructed from source.

| Native start | Iterations | Objective | Native violation | Reference violation |
| --- | ---: | ---: | ---: | ---: |
| Reference | 21 | 150228.14924776586 | 4.044e-13 | 1.771e-13 |
| Declared | 104 | 150228.14924776572 | 4.044e-13 | 1.771e-13 |
| Flat | 64 | 150228.14924776583 | 4.044e-13 | 1.771e-13 |

The largest objective difference is 9.125e-7. Maximum electrical difference is
1.963e-4 pu in generator reactive allocation (`qg[bus-201-4]`); primal equality
is recorded, not required for a potentially nonunique OPF. Four corrupted
PCC P/Q, station DC-power and DC-voltage points fail independent checks.
Native solved points also survive typed PowerIO IR round trips and remain
feasible when supplied as starts.

Six PF/OPF derivative points pass 174 summaries at both relative steps. Maximum
scaled gradient, Jacobian and Hessian errors are 2.896e-7, 1.462e-9 and 2.675e-8,
below the unchanged 1e-6 threshold. Dense structural-zero checks remain active.

## What the extension exposed

- The source declares bus 123 PQ despite active generators. PowerModels'
  `correct_bus_types!` intentionally promotes it to PV. Our PF diagnostics now
  record this in `bus_type_policy`, impose the effective voltage constraint and
  free generator Q without changing the source network. Slack reassignment
  remains unqualified and fails closed. This is reference compatibility, not a
  PowerModelsACDC defect.
- The old second-order finite-difference stencil truncates the stiff
  `thermal_to[301-302]` row: scaled error 2.018e-5 at 1e-5 falls to 1.817e-6 at
  3e-6, the expected quadratic step dependence. Small 0.1 marginal costs also
  suffer cancellation against the roughly 150,000 objective. Fourth-order
  central differences at 1e-4 and 3e-5 resolve both, without relaxing tolerance.
  Sparse accumulation removes repeated zero arithmetic; every dense Hessian
  entry is still compared and existing corrupted-derivative tests still fail.

This adds evidence for these bypasses and three-island topology, not arbitrary
switching, physical rebasing, directional losses, LCC, coupled PF solving, global
optimality or dual sensitivities. Case39 and case67 native solves remain next.
