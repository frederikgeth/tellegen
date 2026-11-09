# Validation results — 2026-11-09

Live independent run: `python3 evals/validation/acdc/run.py /tmp/tacdc-live`.
Julia solved the original case with the pinned environment and settings; the
Rust example evaluated those points using native Tellegen expressions.

| Mode | Native equation rows | Native bound checks | Maximum violation | Native objective |
| --- | ---: | ---: | ---: | ---: |
| PF | 94 | 3 | 9.08162434143378e-14 | 0 |
| OPF | 98 | 66 | 3.141487070479343e-12 | 15693.105053824369 |

Both reference solves returned `LOCALLY_SOLVED` / `FEASIBLE_POINT`. The OPF
objective agrees to the displayed precision. Both native models have 95 named
variables, including AC terminal-flow observables. PF has no OPF operating-limit
checks; its three bounds enforce nonnegative AC valve currents. Maximum
violations are below the 1e-7 gate. Full per-component rows are in
[native-report.json](native-report.json), with independently solved values,
starts, preprocessing snapshots and hashes in [reference.json](reference.json).

Initial equation milestone (`0a9409dd`) Rust validation:

- Full numerical features (`sensitivity,mc-pf,conic,acopf,mc-opf,schema`):
  743 passed, 7 ignored, no failures (including existing ignored/doc tests).
- Workspace excluding Python: 690 passed, 6 ignored, no failures.
- Engine without default features: 152 passed, 3 ignored, no failures.
- Case5 suite with only `acopf`, and with full numerical features: all 5 pass.
  These include multiple mutations per test, and a fixed-angle OPF limit that
  must not become a PF setpoint.
- Full-feature/all-target engine clippy with warnings denied, formatting,
  PowerIO manifest/lock agreement and EPL dependency guard pass.
- `tellegen-wasm` with `conic,synth` checks on `wasm32-unknown-unknown`.

No JavaScript/browser/Python-wheel or publication-package gates were run for
this native working branch; this is not a full `just ci` or release claim.
PowerIO's candidate git revision must be replaced by an appropriate published
release before publication packaging. No new PR was opened, and the existing
PRs remain draft.

The initial benchmark established native equation feasibility at reference
points. The follow-ups below add derivative and native case5 OPF evidence;
physical base transformations and additional cases remain unqualified.
Next work is tracked in [README.md](README.md) and
[PowerIO #819](https://github.com/eigenergy/powerio/issues/819).

## Derivative follow-up

The native derivative example evaluates the same frozen independent PF/OPF
solutions and two deterministic off-feasible perturbations of each. All six
points pass at both finite-difference steps (`1e-5`, `3e-6`).

| Point | Maximum scaled Jacobian error | Maximum scaled Hessian error |
| --- | ---: | ---: |
| PF reference | 5.151e-7 | 3.642e-9 |
| PF perturbed plus | 5.965e-9 | 4.103e-9 |
| PF perturbed minus | 1.351e-8 | 2.087e-9 |
| OPF reference | 8.236e-8 | 4.363e-8 |
| OPF perturbed plus | 6.225e-9 | 4.576e-8 |
| OPF perturbed minus | 9.592e-8 | 8.674e-8 |

Every dense entry is checked, including structural zeros. The worst Jacobian
scaled error is in the disabled PF thermal row `thermal_from[1-5]`, with respect
to `vm[1]`; it falls from 5.151e-7 to 4.636e-8 at the smaller step, consistent
with central-difference truncation. The gate is 1e-6 scaled, not absolute: the
largest absolute Jacobian/Hessian errors are separately retained in the report.
The point reports contain 174 comparisons across all six points, with separate
Hessian checks for physical equation families, the objective and the mixed
Lagrangian. See [README.md](README.md#native-derivative-gate) for the full method.

All five derivative-corruption controls are detected at both step sizes. A
quadratic-cost mutation independently exercises nonzero objective curvature;
case5's original objective is linear. The frozen reference suffices for ordinary
regressions because neither source data nor native electrical equations changed.
The updated integrated live harness was also rerun successfully at
`/tmp/tacdc-derivative-live`; both equation and derivative reports reproduce.

Follow-up validation: 747 full-feature engine tests passed (7 existing ignores),
all 6 case5 integration tests pass with only the `acopf` feature, full-feature
all-target clippy with warnings denied passes, and formatting is clean.
The checked-in [derivative-report.json](derivative-report.json) records all 174
comparisons; the objective-gradient maximum scaled error is 1.428e-10.
This follow-up does not rerun JavaScript/browser/Python-wheel or packaging gates.

## Native OPF and independent cross-feasibility

Live run: `python3 evals/validation/acdc/run.py /tmp/tacdc-opf-live`.
POUNCE 0.15.0 with Feral converged with `Solve_Succeeded` from all three starts,
without recovery attempts. PowerModelsACDC independently accepts each native
point including all constraints and bounds.

| Start | Iterations | Objective | Native violation | Reference violation | KKT error |
| --- | ---: | ---: | ---: | ---: | ---: |
| Reference | 19 | 15693.105053808496 | 7.493e-14 | 7.461e-14 | 2.506e-9 |
| Declared | 75 | 15693.105053808458 | 2.789e-13 | 2.789e-13 | 2.509e-9 |
| Flat | 46 | 15693.105053808495 | 7.362e-14 | 7.350e-14 | 2.506e-9 |

The largest objective difference from the independently solved reference is
1.5911e-8. All compared terminal quantities and branch/valve/station losses agree
within 1.8533e-8 in their pu/radian coordinates; the worst entry is station 3's
transformer-to reactive power. Point equality is recorded, not required for all
nonconvex OPFs. Four corrupted PCC P/Q, DC-power and DC-voltage points fail the
reference checker, with violations from 0.1 to 4.609.

The canonical PowerIO coupled intent requires an explicit profile, preserves
source/effective parameters separately, binds starts to the full source/profile
fingerprint, records solver/recovery settings, and leaves legacy AC-only guards
intact. PowerIO commit `98d5c88f` now owns intent, preparation and stored primal
results;
see [CONTRACT.md](CONTRACT.md). Native coupled PF, general physical projections,
other cases, duals/sensitivities and global optimality are not claimed.

Validation: 749 full-feature engine tests passed (7 existing ignores); all eight
case5 equation/OPF integration tests passed with only `acopf`. Full-feature
all-target clippy, formatting and the PowerIO pin check pass. This is scoped
native validation, not a full JavaScript/browser/Python/package release run.


## Canonical PowerIO migration

Tellegen now pins PowerIO `98d5c88fa7eed18ba537a6c278d0f934dc7d2422` from the
separate `codex/819-acdc-models` branch, based on unchanged reader `49e8eacb`.
The canonical instance, physical quantity records, OPF preparation and stored
primal result replace the Tellegen-only intent prototype. Solver expressions,
starts/recovery, derivative checks and equation validation remain in Tellegen.

Fresh `run.py /tmp/tacdc-model-live` passes with the exact git pin. All three
starts retain the objectives, iterations and residuals shown above, including
independent PowerModelsACDC feasibility and four detected corruptions. The
harness provenance assertion now checks the new PowerIO revision. No
PowerModelsACDC defect was found in this migration.

Each solve also writes a PowerIO IR result (`native-{start}.pio.json`) with all
95 typed physical quantities. Tests serialize/reload these and use them as
canonical instance starts, rechecking native feasibility after unit conversion.
The three local IR outputs are generated evidence, not new vendored fixtures.
The full pinned engine suite passes 749 tests (7 existing ignores); all-target
full-feature clippy, formatting, pin consistency and EPL guard pass.

PowerIO validation and the pre-existing broad clippy failures are recorded in
its model branch's `evals/validation/acdc/RESULTS.md`. All existing PRs remain
draft and unchanged; no new PR or public adapter solve mode is introduced.
