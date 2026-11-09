# Case5 native AC/DC equation qualification

This is the **MATPOWER-extension variant of PowerModelsACDC**, using the
PowerIO reader from draft [PowerIO #822](https://github.com/eigenergy/powerio/pull/822).
PowerModelsACDC's native JSON IR serialization is a separate future reader.
The implementation plan is tracked in [PowerIO #819](https://github.com/eigenergy/powerio/issues/819).

The working branch starts from equipment/ACDC [Tellegen #358](https://github.com/eigenergy/tellegen/pull/358)
at `e814b78549fb5fe48112bc5a72f6732dd115756f`. All PowerIO crates are pinned to
`49e8eacb45b2051de91f917815a26706b8a30a07` using the repository's candidate-pin
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
remain separate. The [experimental contract](CONTRACT.md) stays on this Tellegen
branch; it is not yet an upstream PowerIO instance/preparation API.

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

Central differences use two relative steps, `1e-5` and `3e-6`, with column step
`h=relative_step*max(1,abs(x[column]))`. Every check must satisfy
`abs(analytic-finite)/max(1,abs(analytic),abs(finite)) <= 1e-6` at **both** steps.
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
entries. Maximum scaled errors are `5.150553040079598e-7` for Jacobians and
`8.67399847885586e-8` for Hessians. These checks validate native calculus at the
sampled points; they are not a native solver convergence result, global proof,
or a comparison against a Julia automatic-differentiation implementation.

## Native OPF milestone

`PreparedAcDcOpf` binds an explicit reference profile to immutable preparation,
effective bounds/parameters and a fingerprint of the full source network. It
keeps starts separate from dispatch policy and refuses stale or incomplete
supplied points. The native API is opt-in under `acopf`; no shipping adapter gets
a new solve mode. See [CONTRACT.md](CONTRACT.md) for the prototype and intended
PowerIO ownership boundary.

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

1. Move the tested experimental contract into canonical PowerIO problem,
   preparation and stored-result APIs; retain separate reference/native policies.
2. Extend the derivative, native-solve and independent cross-feasibility gates
   whenever the model or preparation changes; case5's initial gates are complete.
3. Qualify bypassed components, unequal-base transformations, supply filtering,
   multiple AC islands, disabled equipment, direction-dependent losses, and
   further PF controls. Droop/LCC and zero-resistance DC links explicitly refuse
   where unsupported; implemented bypass equations are not oracle-qualified here.
4. Expand to case24/case39/case67 and external large cases. Preserve known
   reference PF infeasibility rather than treating it as a reader failure.
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
