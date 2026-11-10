# Native AC/DC equation and OPF qualification

This is the **MATPOWER-extension variant of PowerModelsACDC**, using the
PowerIO reader from draft [PowerIO #822](https://github.com/eigenergy/powerio/pull/822).
PowerModelsACDC's native JSON IR serialization is a separate future reader.
The implementation plan is tracked in [PowerIO #819](https://github.com/eigenergy/powerio/issues/819).

The working branch starts from equipment/ACDC [Tellegen #358](https://github.com/eigenergy/tellegen/pull/358)
at `e814b78549fb5fe48112bc5a72f6732dd115756f`. All PowerIO crates are pinned to
`552fc413647307322aff7c69688e6b77ff308ad3` (separate PowerIO model branch, based
on unchanged reader `49e8eacb`) using the repository's candidate-pin
mechanism. Existing PRs remain draft. This candidate dependency is not a
published PowerIO release; release packaging must wait for a published version
containing the new owner and guards.

## What this proves

`ReferenceEquations::powermodels_acdc_0101` assembles native `Expr` equations
under the opt-in `acopf` feature. It extends the existing canonical polar AC
model and shares its branch-flow expression builder with converter transformers
and reactors. The reference solution supplies variable values only. No native
formula uses reference effective coefficients, and no PowerIO export is sent
back to Julia in this test.

The check covers:

- AC P/Q balance, reference angle, and both AC branch terminal powers;
- DC branch powers, pole multiplier, and bus balances including withdrawals;
- transformer, reactor, filter, valve P/Q/current, and AC-current loss equations;
- PF generator/converter targets and reference voltage controls;
- OPF AC limits, DC voltages and branch A ratings, valve limits, and the selected
  reference's auxiliary station bounds; the native generation-cost objective.

The public diagnostic interface accepts an exact map of identity-named native
variables. Missing, extra, and nonfinite values fail. `identity-map.json` is an
explicit crosswalk for this fixture between native bus/component identities and
reference table IDs. It includes the two generators at AC bus 1, preventing
an accidental one-generator-per-bus association. It is not a general
PowerModelsACDC JSON reader. All powers are pu on case base MVA, voltages pu,
current on the reference AC base, and angles radians.

`native-report.json` records every checked equation/bound and its violation;
`reference.json` records Julia versions, manifest and case hashes, original and
effective AC/DC parameters, solver options, starts, statuses, objective and
solutions. Tests compare native effective loss/current parameters to the
independently preprocessed reference snapshot as well as evaluating equations.

The initial milestone checked point feasibility; native **case5 OPF solves** are
now qualified from reference, declared-data and independent flat starts. Coupled
PF solves remain future work. The preparation makes an internal AC projection
only while assembling all active equivalent equipment. Ordinary AC-only instance
constructors still reject it. The physical-terminal/reduced-converter solvers
remain separate. The [coupled API](CONTRACT.md) uses canonical PowerIO intent, preparation
and stored primal results from the separate model branch.

## Explicit reference policy

Pinned reference: PowerModelsACDC
[`9ada4082ac087aed649f48fb5f977680b0fe06c7`](https://github.com/Electa-Git/PowerModelsACDC.jl/tree/9ada4082ac087aed649f48fb5f977680b0fe06c7),
version 0.10.1, ACP formulation, `build_acdcpf` / `build_acdcopf`,
`conv_losses_mp=true`; Julia 1.12.6, PowerModels 0.21.7, JuMP 1.32.1,
Ipopt 1.16.0 with MUMPS. `Manifest.toml` fixes the environment.

- PCC and valve P/Q are passive withdrawals. Source `P_g/Q_g` have already
  changed sign in PowerIO; reference solution `pgrid/qgrid` need no sign flip.
  DC power is a withdrawal from the DC bus into the station.
- For base `S` MVA and station AC base `V` kV, loss coefficients are
  `a=LossA/S`, `b=LossB/(sqrt(3)*V)`, `c=LossCinv*S/(3*V^2)`, and
  `pconv+pdc = a+b*iconv+c*iconv^2`. `iconv` is AC valve current,
  constrained by `pconv^2+qconv^2=vmconv^2*iconv^2` and nonnegative in PF/OPF.
- The inverter coefficient is selected in **both** directions. The declared
  rectifier coefficient is retained and reported; directional losses are future
  qualification. Neither PowerIO's declaration nor its original current limit
  is mutated.
- The effective current limit is `max(declared Imax, hypot(Pacrated,Qacrated))`
  with ratings in pu, as in the pinned reference preprocessing. All three case5
  stations change from 1.1 to approximately 1.118034 pu.
- Case5 has 345 kV converter AC bases and 230 kV PCC buses. This profile uses
  the source station per-unit voltage/impedance coordinates directly, matching
  the reference. It does not rebase impedances as if they were supplied in ohms.
  The 345 kV base is used in loss scaling. This is not proof of a physical
  unequal-base projection, network normalization, or equivalence under rebasing.
- Reference auxiliary bounds are ±2 times active/reactive ratings for station
  AC port powers, ±1.2 times active rating for DC port power, filter voltage
  `[Vmmin/1.2, Vmmax*1.2]`, and internal angles ±2π. The report includes them.
- PF uses the selected PCC P/Q settings, converter `Vdcset` for DC voltage
  control, and AC **bus** Vm for AC voltage control. OPF does not impose PF
  targets. PF omits operating limits except nonnegative valve current; its
  objective is zero. Unused declared targets remain preserved.

## Reproduce

From the repository root, with Julia 1.12.6 and Rust installed:

```sh
julia --project=evals/validation/acdc -e 'using Pkg; Pkg.instantiate()'
python3 evals/validation/acdc/run.py /tmp/tellegen-case5-live
```

This solves the original source in Julia, evaluates both points and native
derivatives, runs native OPF from three starts, then checks those native points
in an independently instantiated Julia OPF. No export round trip is involved.
Inspect generated results before replacing checked-in artifacts.

For ordinary regressions without Julia or network access:

```sh
cargo test -p tellegen --features acopf --locked --test acdc_equations
cargo run -p tellegen --features acopf --locked --example acdc_equations
cargo run -p tellegen --features acopf --locked --example acdc_derivatives
cargo run -p tellegen --features acopf --locked --example acdc_opf
```

The threshold is `1e-7` for native constraint/bound violation and `1e-7`
absolute for the OPF objective. Residuals have their equation's units (pu power,
pu voltage, radians, or squared pu quantities); the common small-case threshold
is a qualification threshold, not a universal engineering tolerance.

Negative controls reverse PCC/valve/DC signs, remove the pole multiplier,
substitute the PCC loss base, omit the sqrt(3) linear-current conversion,
change quadratic-current scaling, remove constant losses, reverse filter
susceptance, change tap or PF targets, add DC demand, substitute DC current,
and bind a zero branch rating. Further tests cover malformed points, LCC/droop
rejection, source immutability and continued legacy-consumer refusal.

## Native derivative gate

`ReferenceEquations::check_derivatives` is an exhaustive small-model diagnostic.
It checks the objective gradient, the entire constraint Jacobian, and dense
Lagrangian Hessians reconstructed from the backend's lower-triangular sparse
coordinates. Entries absent from the sparse structure are checked as zeros;
invalid, duplicate or nonfinite derivative coordinates/values fail.

The six points are the independent PF and OPF reference solutions plus two
reproducible off-feasible perturbations of each. In lexicographic native-name
order, `delta = +/-0.03*((i*37 % 101)-50)/50`. Voltage magnitudes and AC current
are multiplied by `1+delta`; all other variables receive an additive `delta`.
No limit clipping or feasibility restoration is applied. Both perturbed points
have material equation violations, so this also exercises derivatives away
from a converged solution.

Fourth-order central differences use two relative steps, `1e-4` and `3e-5`, with column step
`h=relative_step*max(1,abs(x[column]))`. Every check must satisfy
`abs(analytic-finite)/max(1,abs(analytic),abs(finite)) <= 1e-6` at **both** steps.
The stencil is `(8*(f(x+h)-f(x-h))-(f(x+2h)-f(x-2h)))/(12h)`.
Sparse accumulation forms the finite-difference Lagrangian gradient; every dense
Hessian entry, including structural zeros, is still compared.
The report records absolute error separately; the scaled threshold is not an
absolute derivative error bound.

Hessians are compared with finite differences of the checked objective gradient
and constraint Jacobian. Weights are deterministic, signed and nonuniform:
`lambda[row]=(-1)^row*(1+(row*17 % 11))/11`. Separate checks use only one active
equation family at a time: AC balances, AC branch observables, DC branches and
balances, transformer, reactor, filter, valve current, converter losses, affine
controls, and OPF thermal limits. A mixed Lagrangian uses all these weights and
objective factor 0.7; an objective-only check uses factor 1. Inactive OPF rows
have zero Hessian weight in PF. Their Jacobian entries are still checked:
PF has 112 compiled expression rows, of which 94 are active; OPF has 98.
This gives 28 comparison summaries per PF point and 30 per OPF point.

The original case5 generation cost is linear. Its objective Hessian is checked
to be zero, and a native-only quadratic-cost mutation supplies a nonzero
objective Hessian (400 in the modified generator's pu-P coordinate). That
mutation is not a separate PowerModelsACDC solve/parity claim.

Negative controls corrupt actual case5 derivative arrays while retaining the
function-value evaluator: dropped objective-gradient/Jacobian entries, a
spurious Jacobian entry outside sparsity, a reversed valve-current Hessian
cross term, and spurious objective curvature outside Hessian sparsity. Each
must fail at both step sizes. This tests the validator's ability to detect
incorrect derivatives, separately from the electrical equation mutations.

[derivative-report.json](derivative-report.json) contains the six point reports,
including dimensions, sparse counts, dense comparison counts and worst named
entries. Maximum scaled case5 errors are `2.9133484686526554e-10` for Jacobians and
`2.3908678491862956e-8` for Hessians. These checks validate native calculus at the
sampled points; they are not a native solver convergence result, global proof,
or a comparison against a Julia automatic-differentiation implementation.

## Native OPF milestone

`PreparedAcDcOpf` binds an explicit reference profile to immutable preparation,
effective bounds/parameters and a fingerprint of the full source network. It
keeps starts separate from dispatch policy and refuses stale or incomplete
supplied points. The native API is opt-in under `acopf`; no shipping adapter gets
a new solve mode. See [CONTRACT.md](CONTRACT.md) for the PowerIO/Tellegen ownership boundary.

The pure-Rust POUNCE 0.15.0/Feral solver converges from all three case5 starts.
[native-opf.json](native-opf.json) records preparation, starts, final coordinates,
solver options/status/recovery, native residuals and KKT diagnostics.
[cross-check.json](cross-check.json) records each solution's feasibility and
objective in PowerModelsACDC, all terminal quantities and branch/valve/station
loss comparisons, and four reference-validator corruption controls.

All 95 native coordinates map explicitly into the 107-variable reference model.
The remaining reference variables are fixed-demand/zero-curtailment auxiliaries
and unused VSC firing angles; their source-based reconstruction is recorded.
No missing native decision is supplied from the reference solution. Constraints
and bounds, not objective equality alone, establish cross-feasibility.

The largest independent reference violation is 2.789e-13, objective difference
1.592e-8, and electrical-quantity difference 1.854e-8 in the quantity's pu/radian
coordinates. This is local case5 agreement, not a global optimality proof or
blanket full-station/physical-model qualification.

## Remaining model work

1. Review the canonical coupled intent, preparation and stored-result APIs on
   the separate PowerIO model branch; retain explicit reference/native policies.
2. Extend the derivative, native-solve and independent cross-feasibility gates
   whenever the model or preparation changes; case5's initial gates are complete.
3. Qualify bypassed components, unequal-base transformations, supply filtering,
   multiple AC islands, disabled equipment, direction-dependent losses, and
   further PF controls. Droop/LCC and zero-resistance DC links explicitly refuse
   where unsupported. Case24 qualifies its bypasses, disabled filters and three
   AC islands; arbitrary switching and physical projections remain unqualified.
4. Extend the qualified case5/24/39/67 corpus to external large cases. Preserve known reference PF local failures rather
   than treating them as reader failures.
5. Add the natural PowerModelsACDC JSON IR reader separately. No inference of
   JSON support, bindings support, or broader native solve support from case5.

## Fixture provenance

`case5_3_he.m` is byte-for-byte PGLib OPF HVDC at
[`f928f95cde06d035554d92854f03950e074850e0`](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case5_3_he.m),
148 lines / 7,195 bytes, SHA-256
`759606ec335f8706ee8631ff4833c50804dd916afb5b5e682ace4f4f886fd7f3`.
Embedded authorship/references are retained. The upstream `LICENSE` is 31 lines /
1,308 bytes: case data is CC-BY-4.0; its separate MIT section covers software.
The numerical reference/report are derived validation artifacts. The source
case is unmodified; mutation tests operate on in-memory copies.


## Case24 extension

Run `python3 evals/validation/acdc/run.py /tmp/tellegen-case24 --case case24`.
The same pinned model and gates apply; see [case24/README.md](case24/README.md).
Despite its name, this fixture contains 50 AC buses across three asynchronous
areas, 65 generators, 77 AC branches and seven VSC stations. AC source IDs
are nonconsecutive. Its reactor bypasses and disabled filters exercise different
station equations from case5.

The PF diagnostics explicitly reproduce PowerModels' parser repair of PQ/PV
roles by active-generator presence. Bus 123 is declared PQ but becomes PV;
`bus_type_policy` records that choice without changing the PowerIO network.
Selecting a replacement reference bus remains unqualified and refuses.

## Case39 and case67 extension

`cases.json` pins the source filenames/hashes and known reference PF local
failures. Use `--case case39` or `--case case67` with the same runner. Canonical
DC table names and PGLib aliases are accepted; ambiguous dual tables refuse.

[Case39](case39/README.md) passes native OPF equations, three derivative points,
three native starts, complete independent feasibility/objective checks and
physical IR round trips. Its failed reference PF supplies no accepted point.
Only explicitly recorded known PF failures may be skipped; OPF failures and
unexpected reference outcomes still stop the harness.

[Case67](case67/README.md) selects the explicit canonical numerical-reference
policy. Bus 67 keeps its source PV role, while a recorded zero-angle constraint
removes its free common phase. Native diagnostics explicitly align the reference
bus/filter/valve angles together; the independent checker verifies that complete
point against original equations and bounds. The strict default still refuses
this case. No PowerModelsACDC bug was found in the angle-reference investigation.

## Physical units and base conversion

The [physical circuit gate](physical/README.md) independently constructs phase
voltages/currents in kV/kA, converts the same equipment across power and voltage
bases, and checks fresh MATPOWER-extension exports in pinned PowerModelsACDC.
All 36 cases with current-limit repair inactive pass. Six further cases expose
a reference preprocessing limitation: changing voltage base changes the repaired
physical current limit. Three then fail only that bound, consistently in both
implementations. See the reproduction and scope before treating source pu
agreement as physical rebasing support. Runtime equations and pins are unchanged.


The separately selected `DeclaredCurrentAcpVscV1` profile now honours declared
physical current limits. Run the physical harness with `--profile declared-current`
for 108 feasible/binding/violating cases, checked against a Julia model with only
Imax repair undone. Compatibility remains unchanged; see the
[policy contract and evidence](physical/README.md#declared-current-profile--2026-11-10).


Run `physical/run.py OUTPUT --suite bypass` for the
[384-case bypass/unequal-base qualification](physical/BYPASS.md): 288 physical
points pass, while 96 witnesses isolate unsupported unequal-base interpretations.
Production equations, both profiles and the PowerIO pin remain unchanged.

## Explicit physical local-coordinate profile

The [physical-coordinate gate](physical/COORDINATES.md) adds separately selected
weighted transformer bypass and unequal-DC-base equations. PowerIO preparation
records local port/impedance bases and resistance reference side; source
compatibility and declared-current equations remain unchanged. Qualification
uses independent phase-domain physical points and explicitly modified Julia
SI equations. This does not enable generic physical rebasing or native PF.

## Optimized physical-base equivalence

The [local optimization gate](physical/OPTIMIZED.md) checks 128 explicitly equivalent
network representations with 256 native and 256 modified-reference Julia
solves. Both solvers' points pass cross-feasibility, physical bounds and cost
checks on every equivalent base. This covers single-converter synthetic
networks; it does not enable general rebasing or qualify global optimality.

## Three-terminal physical DC mesh

The [mesh gate](physical/MESH.md) adds three independently referenced AC islands,
three VSCs and a triangular DC grid. All 384 local solves and 2,304
cross-formulation/base checks pass, including binding line/current limits
and economic redispatch. The independent reference uses documented equivalent
row normalization and constant-auxiliary presolve; original auxiliary rows
and bounds are restored for validation. No production model or pin changes.


## Connected AC/DC networks and outages

The [connected/outage gate](physical/CONNECTED.md) adds a physical AC triangle,
AC/DC line outages, separate AC/DC islands and a converter outage. It covers
32 physical networks on four bases, 512 local solves, 3,072 cross-formulation/base
checks and 768 corruption controls. Source roles and inactive declarations are
retained; numerical references follow AC connectivity only. Native tests reject
stale starts and an unsupplied isolated load. The report documents the explicit
idle-loss initialization and Ipopt interior-push settings, with unchanged
acceptance thresholds and no confirmed upstream electrical-equation bug.

Run `CARGO_TARGET_DIR=/tmp/tacdc-connected-target python3 evals/validation/acdc/physical/connected.py /tmp/tacdc-connected-live`.


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


## Case67 PF reference/dispatch and droop contract — 2026-11-10

The [case67 contract study](pf/case67/README.md) explicitly promotes isolated
bus 67 from PV to reference, retains source generation dispatch and converter
controls, and selects shared reactive-injection increments. Type-3 DC droop uses
withdrawal-positive Pdc, Pdcset/baseMVA and the retained pu-voltage/pu-power slope.
Changing converter 9's PCC withdrawal from 800 to 810 MW makes its island's
generator supply 810 MW, demonstrating the selected PF dispatch semantics.

Eight variants include target/slope changes, a grid controlled only by droop,
a DC-line outage and tight ratings. **23 reference solves and 23 native
electrical/control residual checks pass**, with 492 scalar droop witnesses,
351 reference corruptions and 328 native corruptions. One outage declared-start
trial reaches the iteration limit; flat and perturbed starts succeed. Failed
iterates remain recorded. Maximum electrical residual is 1.037e-12; maximum
native control residual is 3.730e-14. Rating violations remain separate.

This is a contract gate, **not native droop PF solver qualification**. Both
existing PF profiles still refuse droop. No production profile, pin or schema
changes. Full suite: **797 passed, 7 ignored**; lint/build/format/pin/EPL guards
pass. Next: the opt-in native DC-power-droop/shared-voltage profile, IR rejection,
control/island admission tests, derivatives and independent native/reference
solves. No new PMACDC equation bug is established.


## Native case67 DC-power-droop PF — 2026-11-10

The [opt-in droop PF profile](pf/DROOP.md) implements the case67 contract with
PowerIO **70025e87**. Withdrawal-positive DC droop replaces fixed PCC P only when
selected; shared reactive injection allocation remains explicit. Each DC island
needs fixed-voltage or positive-droop authority. Old profiles retain their
refusals; zero/negative/nonfinite-inverse slopes, deadbands, valve-power droop,
duplicate fixed-Vdc controllers and uncontrolled islands refuse.

Eight variants pass **24 native + 23 reference solves, 47 cross-model checks and
443 corruptions**. All native starts converge in five iterations. Maximum
native residual is 3.664e-13 and electrical difference 2.782e-10. One reference
outage/declared trial fails locally, while its native point satisfies reference
equations; the failure is retained. Six derivative points, 11 preparation
refusals, source retention, IR rejection and stale starts pass. Original case67
still needs the explicitly selected bus-67 reference; source Pg is retained.

Full suite: **801 passed, 7 ignored**; build/lint/format/pin/EPL pass. Old case5
passes 48 solves/checks and 96 corruptions, with eight unchanged fingerprints.
All 42 compatibility exports/records are unchanged. PowerIO passes 255 affected
tests, current/frozen IR checks, changed-library Clippy and documentation checks;
known unrelated float-comparison failures remain in three broad lint targets.

We probably want [proper saturation later](pf/SATURATION.md). Track explicit
limit-triggered control transitions, redistribution and loss of island voltage
support as a separate policy/qualification task. Current PF reports violations
without clipping or switching; baseline case67 has 13, tight ratings 49. This is
not operational-feasibility evidence. No new PMACDC equation bug is established.
