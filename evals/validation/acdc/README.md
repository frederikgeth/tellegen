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

This is point-feasibility qualification, **not a native coupled PF/OPF solve**.
The diagnostic constructor makes an internal AC projection only while assembling
all active equivalent equipment. Ordinary AC-only instance constructors still
reject it. The existing physical-terminal/reduced-converter solvers are separate.

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

This independently solves the original licensed source twice and evaluates both
points through Tellegen's native expressions. No export round trip is involved.
Inspect generated results before replacing the checked-in reference/report.

For ordinary regressions without Julia or network access:

```sh
cargo test -p tellegen --features acopf --locked --test acdc_equations
cargo run -p tellegen --features acopf --locked --example acdc_equations
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

## Remaining model work

1. Move the qualified profile/point semantics into PowerIO problem/preparation
   contracts; keep the reference policy distinct from native/directional models.
2. Check Jacobians/Hessians against finite differences, then perform native
   coupled OPF solves and reference cross-feasibility, limits and objective checks.
3. Qualify bypassed components, unequal-base transformations, supply filtering,
   multiple AC islands, disabled equipment, direction-dependent losses, and
   further PF controls. Droop/LCC and zero-resistance DC links explicitly refuse
   where unsupported; implemented bypass equations are not oracle-qualified here.
4. Expand to case24/case39/case67 and external large cases. Preserve known
   reference PF infeasibility rather than treating it as a reader failure.
5. Add the natural PowerModelsACDC JSON IR reader separately. No inference of
   JSON support, bindings support, or native solve support from this benchmark.

## Fixture provenance

`case5_3_he.m` is byte-for-byte PGLib OPF HVDC at
[`f928f95cde06d035554d92854f03950e074850e0`](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case5_3_he.m),
148 lines / 7,195 bytes, SHA-256
`759606ec335f8706ee8631ff4833c50804dd916afb5b5e682ace4f4f886fd7f3`.
Embedded authorship/references are retained. The upstream `LICENSE` is 31 lines /
1,308 bytes: case data is CC-BY-4.0; its separate MIT section covers software.
The numerical reference/report are derived validation artifacts. The source
case is unmodified; mutation tests operate on in-memory copies.
