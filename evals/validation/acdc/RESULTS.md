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

## Case24 extension — 2026-11-10

The same branch and PowerIO pin now qualify case24's native equations,
derivatives and three OPF starts against the independent Julia formulation.
See [case24 results and provenance](case24/README.md) and its machine-readable
[summary](case24/summary.json). Maximum native/reference solved-point violations
are 4.044e-13 / 1.771e-13; objective difference is 9.125e-7.

This exposed a missing reference PF bus-role repair (PQ→PV at bus 123) and
finite-difference truncation/cancellation in our checker. Both are addressed
explicitly, with the same 1e-6 derivative threshold. No PowerModelsACDC defect
was found. The six-point case5 harness was rerun as a regression: gradient,
Jacobian and Hessian scaled errors are now <=2.004e-11, 2.914e-10 and 2.391e-8
with the fourth-order stencil; case5 solve and independent feasibility results
are unchanged. Earlier derivative numbers above describe the prior stencil.

Final validation: 751 full-feature Tellegen tests passed, 7 ignored, no failures
(`acopf mc-opf conic schema`); all-target clippy with those features, formatting,
PowerIO pin consistency and EPL dependency guards passed. Case24 integration
tests cover source preservation, recorded bus-role repair, all three native OPF
starts and canonical physical-result serialization/reload.

## Case39 qualification and case67 reference gap — 2026-11-10

[Case39](case39/README.md) now passes original-source OPF equations, three
OPF derivative points at both steps, three native starts, independent reference
feasibility/objective checks, four corruption controls and physical IR reloads.
The native objective is approximately 138063.517292759; the largest reference
violation is 5.508e-12 and objective difference 2.110e-6. All 456 native and
529 reference variables are accounted for. Its failed PF remains explicit
(`LOCALLY_INFEASIBLE`, violation 0.4694775713833841), with no PF acceptance claim.

[Case67](case67/README.md) remains **unqualified natively**. Original-source
reference OPF succeeds (122253.02388749263, violation 6.822e-13); PF again returns
`LOCALLY_INFEASIBLE` (0.5991975989165781). Native canonical preparation refuses
singleton AC bus 67 because it has no declared reference. Shifting its bus and
converter-9 internal angles by +/-0.05 rad preserves all reference constraints,
bounds and objective at the solved point. This confirms a free phase in the
reference, not a PowerModelsACDC bug. The documented next step is explicit
canonical numerical-angle-reference policy and gauge-aware comparison; source
bus roles, model semantics and PowerIO pin remain unchanged here.

The runner accepts canonical DC table names as well as PGLib aliases, records
known PF failures separately, refuses failed OPF references and removes stale
success summaries before reruns. Full reports are at the requested output path;
compact accepted case39 evidence and case67 refusal/gauge evidence are vendored.

Validation: 755 full-feature Tellegen tests passed (7 ignored, no failures);
all-target clippy passed with `acopf mc-opf conic schema`. The fresh case5 full
reference comparison still passes with unchanged mathematical results. Four
new tests cover case39 reference equations/three starts/IR reload, case67 refusal
and source preservation, and rejection of a failed OPF masquerading as a
skippable PF result. No native runtime equations changed in this extension.

## Explicit angle policy and case67 qualification — 2026-11-10

Tellegen now pins PowerIO model commit **552fc413**. Coupled intent selects a
persisted numerical-angle-reference policy: strict `Declared` by default, or
opt-in `LowestBusIdPerIsland`. Preparation preserves declared references and
records added anchors by source identity without changing source/PF roles.
The version-3 native manifest/fingerprint includes that policy and anchor set.
Reference alignment is a separate, bound-checked operation; supplied starts are
otherwise exact. Ordinary AC-only and unqualified PF guards remain.

[Case67](case67/README.md) now passes the complete native/reference OPF gate.
Bus 67 remains source PV while its numerical angle is fixed to zero. The
independent oracle is aligned by -0.021358541264414423 radians at bus 67 and
converter 9's filter/valve angles; the original reference model accepts this
point at 6.822e-13 with unchanged cost. All 739 native/859 reference coordinates
are accounted for, without filling missing native decisions from oracle values.

Reference/declared/flat native starts converge in 14/24/69 iterations. Maximum
native/reference violation is 6.739e-11, objective difference 4.531e-8, aligned
electrical difference 6.454e-8 and native KKT error 6.558e-9. Three derivative
points pass all 90 summaries at two steps without relaxing tolerance. Five
corruption controls are detected, including a bus-only angle shift. Physical
IR reloads preserve the selected policy and feasible complete primal points.

Case67's reference PF local failure remains explicit and unqualified; no
PowerModelsACDC bug was found. The previous gap is resolved for this OPF profile,
not a change to reference PF semantics or a global/physical-equivalence claim.

Validation: 758 full-feature Tellegen tests pass (7 ignored), all-target clippy,
formatting, PowerIO pin and EPL guards pass. PowerIO's full default workspace run
plus the corrected IR-reference documentation test covers 3,631 passing tests;
55 C ABI and seven focused coupled tests pass. Its changed libraries/focused
clippy and C ABI/Python configurations pass; three broad configurations retain
the previously documented unchanged exact-float warnings. Published IR schemas,
reader #822 and the draft Tellegen PR heads remain unchanged.

Fresh full pipelines for case5, case24 and case39 also pass with the new pin,
strict declared-reference policy, unchanged tolerances and mathematical results.
Their reference points, summaries and existing case5 reports have been refreshed.


## Physical-unit and base-conversion gate — 2026-11-10

The [physical circuit qualification](physical/README.md) adds three Rust tests
and a reproducible independent Julia harness. Thirty-six representations of the
same analytical circuits cover 50/100/250 MVA, three unequal PCC/converter/DC
voltage-base triples, both power directions and one/two symmetric DC poles.
Native PF/OPF equations, physical losses, OPF cost/bounds and canonical result
conversions agree. Seven incorrect conversions are detected; unsupported generic
normalization and AC-only instance construction continue to refuse.

All 42 fresh MATPOWER-extension exports were independently parsed, preprocessed
and instantiated in pinned PowerModelsACDC: 39 points pass, and three fail only
the expected current bound. Every other constraint/bound is within 1.137e-13;
physical objective differences are zero. All 22 native/23 reference coordinates
per case are accounted for; 42 reference current-corruption controls fail.

The six additional cases confirm a limitation in upstream current-limit repair:
the same 0.262488 kA physical point is accepted with the 200 kV converter base
(adjusted limit 0.408248 kA), but rejected at 400 kV (0.204124 kA). This occurs
at all three power bases. The equations stay physically equivalent; the
nominal-1-pu repair changes the feasible set. Native compatibility is preserved,
and arbitrary voltage-base invariance is explicitly not qualified. This finding
is separate from the earlier case67 angle-reference investigation.

Validation: 761 full-feature Tellegen tests pass (7 ignored), all-target clippy
with warnings denied, formatting, PowerIO pin and EPL guards pass. No production
equations, public profiles or dependency pins changed. The four existing corpus
solve/derivative results remain the evidence for native optimization; this new
gate evaluates analytical points and does not claim optimality or PF solves.
The Julia environment remains validation-only. Existing PRs remain draft.


## Declared-current policy — 2026-11-10

PowerIO **896fef02** adds a separate `DeclaredCurrentAcpVscV1` profile; the
reference compatibility profile remains unchanged. Tellegen records the derived
current-limit policy, distinguishes profile fingerprints and refuses applying
this OPF limit policy to PF diagnostics. The new profile honours declared limits,
including zero or undersized values, without increasing them to fit P/Q ratings.

[108 physical cases](physical/declared-current.json) pass the expected acceptance
checks: 36 strictly within the current limit, 36 at it and 36 exceeding it by
0.212488094968134 kA. Limits/violations are invariant across all tested power and
voltage bases, both flow directions and pole counts. Native and explicitly
modified Julia models agree; the sole Julia modification restores source Imax
after preprocessing. Every other constraint/bound is within 1.137e-13, objective
differences are zero, and all 108 corruption controls are detected. Typed IR
round trips preserve the policy and exact supplied points, including infeasible
points. Profiles reject each other's fingerprinted starts.

All prior 42 compatibility-circuit records and source hashes reproduce exactly.
A fresh full case5 pipeline passes with unchanged model fingerprint. The full
Tellegen suite passes **762** tests (7 ignored), including all four corpus cases;
all-target Clippy with warnings denied, pin/EPL/fmt guards pass. PowerIO affected
problem/matrix/facade/schema suites pass 493 tests; its full Clippy matrix retains
only the previously documented baseline failures. Frozen 552fc413 profile decoding
rejects the new token and still accepts compatibility. Only the unreleased schema
changed. PR #300/#358/#822 remain draft at their previous heads.

This qualifies declared-current bound semantics, not a full physical equipment
profile, arbitrary rebasing, optimum invariance or native coupled PF. The next
physical gate is bypass and unequal-base semantics; see [scope/reproduction](physical/README.md).


## Bypass and unequal-base gate — 2026-11-10

The [384-case physical qualification](physical/BYPASS.md) covers every
transformer/reactor/filter flag combination, both directions, one/two poles and
three power bases. 288 qualified points satisfy both native profiles and the
unmodified pinned Julia model. 48 unequal-PCC/filter-base transformer-bypass
witnesses fail only voltage equality; 48 unequal-DC-endpoint-base witnesses fail
only both DC Ohm-law equations. All other constraints/bounds and objective
differences are within 1.137e-13. Julia independently locates each expected
failing constraint; all 384 current-corruption controls specifically fail the
previously satisfied current equation.

Four new Rust tests cover the complete case matrix, physical conservation,
source IR preservation, disabled-parameter retention/re-enabling, ineffective
single-resistance rescaling and metadata-only base edits. Derivative checks pass
for all eight station combinations at two finite-difference steps. Full-feature
validation: **766 passed, 7 ignored**; all-target Clippy, formatting, pin/EPL guards
pass. The existing four-case native regressions pass, and all prior 42/108
physical-case record fields reproduce exactly.

No production equations, profile meanings, dependency pins, PowerIO source or
PR heads changed. These are limitations of a physical interpretation of the
source-coordinate model, not a native/reference discrepancy. The next gate is
an explicit physical-coordinate contract and separately selected weighted
bypass/DC formulation, before enabling general physical rebasing. See the
[compact evidence](physical/bypass-qualification.json) and reproduction commands.

## Physical local-coordinate profile — 2026-11-10

PowerIO is pinned to `ba1f9fc90de768e53aaace1c21e68b0d64759713` on the existing
model branch. `PhysicalCoordinatesAcpVscV1` records local AC port/impedance bases
and from-side DC resistance bases, uses weighted bypass/DC equations and keeps
declared current limits. The existing profiles retain their source equations,
losses, bounds and fingerprints. No generic rebasing API is enabled.

All 384 independent physical points pass native equations/bounds (`4.963e-14`)
and explicitly modified Julia SI equations (`7.276e-12`). This includes all
96 prior unequal-base witnesses. All 108 physical-current cases have expected
outcomes: 72 feasible/binding, 36 failing only the declared current bound.
Native checks cover 384 IR round trips, 48 correct/wrong resistance-reference
branch reversals, 16 derivative cases and metadata corruption. Full scope,
compact evidence and reproduction are in [COORDINATES.md](physical/COORDINATES.md).

Compatibility evidence reproduces every recorded field for the previous 42/108
physical cases and all 14 source-bypass groups. A fresh case5 pipeline retains
the old compatibility fingerprint. All four native corpus regressions pass;
full-feature validation is 769 passed, 7 ignored, with all-target Clippy and
fmt/pin/EPL checks passing. PowerIO's 496 affected tests and changed-code Clippy
pass; only its previously documented broad-matrix baseline float warnings
remain. Native coupled PF, optimizer invariance, directional losses and physical
pole/return topology are still separate gates. Draft PR heads are unchanged.

## Optimized physical-base equivalence — 2026-11-10

The [optimization follow-up](physical/OPTIMIZED.md) qualifies 32 synthetic
physical networks represented on four combinations of MVA/local voltage bases.
All eight station flag combinations, both directions and one/two poles pass.
There are 256 native and 256 independently initialized modified-reference Julia
local solves, 3,072 cross-formulation/base checks, 512 corruption controls and
direct phase-domain checks of both solvers' points. Maximum native–Julia cost
difference is 4.795e-9; largest Julia cross violation is 1.747e-10. Physical
variable bounds are identical across bases and match independently parsed Julia
bounds. Global optimality and primal uniqueness are not inferred.

All 30 affected AC/DC regression tests pass, including two new optimization
regressions and the four existing corpus cases. All-target numerical-feature
Clippy, fmt/pin/EPL guards pass. The extracted physical-reference helper
reproduces the prior 384-point evidence exactly. No production model, profile,
PowerIO pin, schema or draft PR head changed. Multi-converter/meshed physical
networks remain the next validation gate; no general rebasing API is enabled.

## Multi-converter physical DC mesh — 2026-11-10

The [mesh qualification](physical/MESH.md) covers 24 physical networks on four
base representations: three AC islands, three VSCs and a DC triangle; two
station configurations, both economic transfer directions, 1/2 poles and
unconstrained/line-limited/current-limited regimes. All 192 native and 192
independent Julia local solves pass, along with 2,304 cross-formulation/base
checks and 576 corruption controls. Max cost difference is 1.459e-8; max Julia
cross violation in original units is 3.007e-10. Physical bounds, direct phase
equations, loss balance and economic effects of active limits are checked.

Reference-only numerical failures were investigated explicitly. Final validation
uses equivalent SI-row normalization and exact presolve of declared-constant
auxiliaries, restoring all original auxiliary equalities/bounds for acceptance.
All starts use identical recorded settings; no failed trial is counted as a
pass and no acceptance threshold is relaxed. This is a conditioning finding,
not a confirmed electrical-equation bug in unmodified PowerModelsACDC.

All 32 affected AC/DC tests pass, including six mesh derivative cases and
identity-preserving row reorderings. All-target Clippy and fmt/pin/EPL checks
pass. No production equation, profile, schema, PowerIO pin or draft PR head
changes. Connected-AC-network interactions and outage/islanding checks remain
subsequent gates; native PF, generic rebasing and physical pole wiring remain
unqualified.

The optimization and mesh examples explicitly require `acopf`, fixing the
previous optimization example's missing feature gate. Default-feature
`cargo check -p tellegen --all-targets --locked --offline` also passes.


## Connected AC/DC networks and outage/islanding — 2026-11-10

The [connected qualification](physical/CONNECTED.md) covers eight topologies,
both economic transfer directions, one/two poles and four physical-base
representations: 256 native + 256 independent Julia local solves, 3,072
cross-formulation/base checks, 1,024 native base checks and 768 corruption
controls pass. Maximum cost difference is 1.920e-8 and maximum Julia cross
violation in original units is 1.322e-9. Direct AC pi/station/DC physics and
nodal/global loss balances pass. Separate AC-island references remain necessary
even with DC connectivity; inactive equipment remains in source/IR but leaves
optimization coordinates. Stale starts and an unsupplied load island are rejected.

Initial idle-converter trials exposed numerical sensitivity. Declared idle-loss
starts and uniform small Ipopt interior pushes qualify the final matrix without
relaxing limits/tolerances. No confirmed upstream equation bug was found. All
36 affected AC/DC tests, all-target Clippy, default all-target compilation and
fmt/pin/EPL guards pass. The shared Julia helper reproduces the prior mesh report
byte-for-byte. No production model, compatibility profile, schema, PowerIO pin or
draft PR head changes. Directional loss semantics, native PF controls, generic
rebasing and physical pole wiring remain separate gates.


## Directional loss contract — 2026-11-10

The [loss-contract gate](physical/LOSSES.md) checks 1,728 physical witnesses
across asymmetric/equal coefficients, reversal and idle/zero-power boundaries,
full/bypassed stations, 1/2 poles and nine bases. All 504 expected inverter-only
loss discrepancies equal `(Cselected-Cinv)*I²`; other equations and 1,728
current-corruption controls pass. This is pointwise characterization, not a
new directional OPF solver. All 39 affected AC/DC tests and build/lint guards pass.

PowerModelsACDC intentionally uses Cinv in both directions; its parser docs
incorrectly label that field unimplemented. A separate MATACDC 1.0 source audit
finds inconsistent mode labels and an exact-zero quadratic omission; neither
is silently adopted. PCC, valve and DC power signs differ near idle operation,
and a hard valve-P switch has a finite jump at nonzero Q. The next gate is an
explicit fixed-mode physical loss policy and branch-constrained optimization,
with automatic reversal kept separate. Existing profiles, pin and draft PR
heads remain unchanged.


## Fixed-mode physical OPF — 2026-11-10

The [fixed-mode gate](physical/FIXED-MODES.md) adds identity-keyed rectifier/inverter
choices in PowerIO `27aae3b3`, selected loss coefficients, valve-P sign bounds and
mode-aware preparation fingerprints. Existing profiles and draft PR heads retain
their behavior. All 384 local solves, 2,304 cross-formulation/base checks and 1,728
boundary witnesses pass, including 648 off-domain sign violations and 32 derivative
cases. The new profile uses stricter convergence near zero current; no acceptance
threshold or physical limit is relaxed. Full Tellegen tests: 783 passed, 7 ignored.
Native coupled PF controls are next; automatic mode reversal remains separate.


## Native coupled PF controls — 2026-11-10

The [case5 PF gate](pf/README.md) adds explicit canonical PF intent/results and
source-pu/Cinv control semantics at PowerIO `e5a7c8b8`. Native damped Newton solves
P/Q, P/Vac, Vdc/Q and Vdc/Vac cases with explicit equal-increment generator
allocation. Ratings are reported after solving; costs and OPF dispatch bounds do
not enter PF. Eight case5 variants pass **24 native + 24 independent PMACDC
solves, 48 bidirectional checks and 96 corruption controls**. Maximum electrical
difference is 5.744e-10; phases are compared modulo 2pi with raw windings retained.
The report documents exact reference setpoint presolve and earlier failed Ipopt
trials. No new upstream equation bug is established.

Full Tellegen suite: **787 passed, 7 ignored**; build/lint/pin/EPL guards pass.
The 42 compatibility exports/native records and old case5 fingerprint remain
unchanged. Next: case24/larger PF and connected control/islanding qualification.
Physical/fixed-mode PF, saturation and droop remain separate gates. Existing PRs
remain draft; the work stays on the two development branches.


## Case24 PF controls and static islands — 2026-11-10

The [case24 PF gate](pf/CASE24.md) retains the existing control policy and PowerIO
pin. **Unmodified case24 is rejected** for overlapping generator/converter voltage
control at bus 215. An explicit variant puts converter 6 on its retained PCC Q
setting. Eleven positive control/outage/island variants pass **66 solves,
66 bidirectional checks and 132 corruption controls**; the three baseline points
also satisfy unmodified original PMACDC equations (maximum violation 2.729e-13).
Nine case24 control refusals, two larger-source refusals, inactive-coordinate
exclusion and stale post-outage starts are checked. Maximum electrical difference
is 2.105e-9.

A native Newton stall near an isolated converter's idle point was resolved with
an exact nonnegative-current retraction candidate, preserving all equations and
acceptance thresholds. Its use is reported. No new PMACDC equation bug is
established. The case5 rerun passes all 48 solves/48 checks/96 corruptions with
unchanged fingerprints. Full suite: **790 passed, 7 ignored**; lint/build/pin/EPL
guards and 42 unchanged compatibility records pass. Existing PRs remain draft.

Original case39 lacks a DC-voltage controller; case67 lacks an AC-island reference
and also needs droop semantics. Next is an explicit control-policy decision for
these source cases and shared voltage authority, not a silent source repair.


## Shared AC-voltage PF allocation — 2026-11-10

The [shared-voltage gate](pf/SHARED-VOLTAGE.md) adds a required opt-in profile at
PowerIO `dbc52de6`, retaining old profiles and source declarations. Generators and
voltage-controlling converters share equal reactive **injection** increments;
converter withdrawal changes have the opposite sign. Native preparation emits
one Vm target per bus and explicit allocation rows. Original case24 now solves
without changing its controls. Converter-only/mixed groups, unequal declarations,
inactive controllers, tight ratings, signs, derivatives and row permutations are
qualified against PMACDC plus independently formed allocation rows.

**48 solves, 48 bidirectional checks, 123 corruptions and three original-source
checks pass.** Maximum electrical difference: 1.955e-10. Original case24 satisfies
unmodified reference equations within 2.729e-13. Old case5 passes 48 solves/48
checks/96 corruptions with eight unchanged fingerprints; 42 compatibility records
are unchanged. Full Tellegen suite: **793 passed, 7 ignored**, all build/lint guards
pass. Frozen e5a7c8b8 IR accepts the old PF profile and rejects the new profile in
instances/results. Published schemas and draft PR heads stay unchanged.

Next: a labeled case39 DC-voltage-controller variant. Case67's PF reference and
droop semantics remain separate; no source control is silently repaired.


## Case39 PF with explicit controller and fixed dispatch — 2026-11-10

The [case39 study](pf/CASE39.md) explicitly selects a DC-voltage controller and
fixed generator Pg from the previously qualified source OPF. No solved voltages,
Qg, converter outputs or PF points seed the independent solvers. Original case39
still refuses; selecting a controller alone produces six failed local trials in
each solver, which does not establish infeasibility.

Eight variants pass **24 native solves, 23 reference solves, 47 cross-model checks
and 92 corruptions**. The reference `dc_voltage/flat` trial fails locally; the
same case succeeds from declared and perturbed starts, and all 24 native points
satisfy reference equations. Failed-trial results remain in the evidence. Maximum
electrical difference is **4.068e-9**. Baseline rating violations remain reported;
this is equation qualification, not operational feasibility.

Full suite: **795 passed, 7 ignored**; build/lint/format/pin/EPL checks pass.
Case5 passes 48 solves/checks and 96 corruptions with eight unchanged fingerprints;
42 compatibility exports/records remain byte-identical. Production profiles,
PowerIO pin dbc52de6 and draft PR heads are unchanged. Next: case67's explicit
PF reference/dispatch and droop contract. No new PMACDC equation bug is established.
