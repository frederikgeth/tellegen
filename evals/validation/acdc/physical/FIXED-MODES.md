# Fixed valve-loss modes — 2026-11-10

`FixedModePhysicalAcpVscV1` implements the explicit branch policy established by
[the directional-loss witnesses](LOSSES.md). Tellegen pins PowerIO
`27aae3b3313b17916abd84cc33fe6a2dd459ec0b` on the existing model branch. The
MATPOWER-extension reader PR and both Tellegen PRs remain draft at their prior
heads. The natural PowerModelsACDC JSON variant remains separate.

## Contract and compatibility

Construct `powerio::AcDcOpfInstance::from_network_with_loss_modes` with the new
profile and one `AcDcStationLossMode` for every active converter, keyed by its
persistent component ID. `AcDcValveLossMode::Rectifier` selects Crec and valve
AC withdrawal P >= 0; `Inverter` selects Cinv and P <= 0. These sign constraints
intersect the source valve-P bounds; an empty intersection refuses construction.
PCC and DC signs do not select the mode. The existing physical local-base,
weighted bypass/DC and declared-current policies apply unchanged.

At P = 0, the declared mode still selects its loss curve. Reactive current can
therefore give different zero-active-power losses on the two branches. At I = 0,
the constant loss remains. No automatic switching, smoothing, coefficient
averaging, discrete-mode optimization or PF control is introduced.

Missing, duplicate, unknown or inactive assignments are refused. Canonical
sorting makes list order irrelevant, while changing a station's mode changes
preparation identity even when Crec = Cinv. Instance/initial-point/solution IR
retains the choices. Preparation preserves modes when stripping initial points,
and stale starts from another mode fail closed. Only the unreleased PowerIO
schema changes. Its old profile decoder rejects the new required token.

All three earlier profiles retain their equations, bounds, solver settings,
profile tokens, omitted-field IR shapes and preparation fingerprints. In
particular, the original compatibility profile still deliberately uses Cinv in
both directions and the reference Imax repair. New modes cannot be supplied as
an override to those profiles.

## Optimization gate

The [three-terminal DC mesh](MESH.md) supplies 24 physical networks: two station
flag/coefficient arrangements, cheap generation at bus 11 or 13, one/two poles,
and unconstrained, binding DC-line or binding valve-current regimes. Each has
four representations with different MVA, AC port and DC-node bases. Coefficients
are `[Crec,Cinv]=[0.3,1.3]` or `[1.3,0.3]` ohms. The cheap station is declared
rectifier; the other two are declared inverter. Both branches remain free to
dispatch inside their signed domains. The economic data and original line and
current limits are unchanged; constrained cases must have a higher cost.

Each representation is solved from the zero-transfer/local-generation start and
a deterministic perturbation. Julia independently parses fresh MATPOWER-extension
exports with pinned PowerModelsACDC 0.10.1 (`9ada4082`). As in the preceding
physical gates it restores declared Imax, substitutes SI bypass/DC rows,
normalizes those rows for optimization and presolves proven constant/unused
auxiliaries, restoring their original rows/bounds for validation. This gate also
verifies the original loss row uses Cinv, replaces it with the declared branch,
and intersects valve-P bounds with its sign. Every numerical variable and
physical bound is accounted for. This is explicitly a modified reference,
not a claim that stock PowerModelsACDC implements directional losses.

Independent phase-domain reconstruction checks station phasors, currents,
terminal powers and losses, AC/DC nodal balance, DC Ohm laws and total network
loss. Both solvers' points must satisfy all four equivalent-base models with the
same objective. Three corruption controls per reference solve change DC voltage,
PCC power or swap station DC injections. Global optimality, unique voltage
solutions and global infeasibility are not inferred.

## Zero-current numerical qualification

Some line-limited fixed-mode optima place a receiving valve at P = I = 0.
The squared current equation then has vanishing gradients. Initial default
native settings produced a physical current error of 1.39e-6 kA despite a small
algebraic residual; a later tighter-primal trial still failed the existing
unscaled KKT acceptance at about 2e-6. Overly tight overall tolerance also
produced `SearchDirectionBecomesTooSmall`. None of these trials counts as a pass
or an infeasibility proof, and they do not establish a PowerModelsACDC bug.

The new profile alone uses `tol=1e-9`, `constr_viol_tol=1e-12`,
`dual_inf_tol=1e-7`, `acceptable_iter=0`, `bound_push=1e-8` and `bound_frac=1e-8`.
Explicit unscaled dual convergence prevents multiplier scaling at zero current
from hiding a poor KKT residual. Native options are recorded in the compact
report. All previous profiles keep their existing settings. The Julia reference
uses the prior mesh options without additional changes.

Acceptance is unchanged: constraint residual <1e-7, native KKT <=1e-6,
physical reconstruction <1e-6 in kV/kA/MW/MVAr, and objective agreement within
`1e-6 + 1e-8*|cost|`. No limits, sign domains or physics were relaxed.

## Results

All **192 native + 192 Julia local solves** pass. The 96 representations give
**2,304 cross-formulation/base checks**, another 768 native base checks and
576 reference corruption controls.

| Metric | Maximum |
| --- | ---: |
| Native own-model violation | 8.434e-13 |
| Native KKT error | 9.758e-10 |
| Native physical residual | 4.270e-8 |
| Reference own-model violation, original units | 2.274e-11 |
| Cross-base violation in reference, original units | 3.411e-11 |
| Reference points in native models | 2.727e-11 |
| Reference physical residual | 6.952e-7 |
| Native–reference objective difference | 1.296e-10 |
| Cost range across solvers, starts and bases | 2.024e-10 |

The separate boundary gate checks **1,728 witnesses**: asymmetric/equal
coefficients, forward/reverse/PCC-sign traps, positive/negative near-zero P,
zero P with reactive current, zero current and DC-idle operation, full/bypassed
stations, one/two poles and nine bases. Selected loss equations pass throughout;
**648 deliberately off-domain points** violate the valve sign constraint. The
count uses >1e-12 MW to exclude roundoff at analytically zero P; the signed
violations are compared without clipping. Both declared zero-P branches are
feasible within tolerance and preserve the 6.4 kW difference at 0.08 kA reactive
current. DC-idle loss is supplied by positive valve AC power, so only the
rectifier branch admits that witness. There are 1,728 current-corruption controls
and 32 native derivative cases. Boundary witnesses are point checks, not 1,728
additional OPF solves. Some optimized mesh cases independently reach zero current.

Maximum other/selected-branch reference residual is 7.276e-12; native/reference
sign-violation disagreement is 5.552e-17 MW. The 504 expected stock Cinv-only
loss discrepancies remain independently characterized, not silently reclassified
as reference bugs.

The [optimization report](fixed-mode-qualification.json) and
[boundary report](fixed-boundaries-qualification.json) are each below 100 KiB.
They retain base configurations, modes, coefficients, dispatch/limit evidence,
solver settings, source/report hashes and scope. Full cases, points and logs stay
outside the checkout.

The full Tellegen suite with `acopf mc-opf conic schema` passes: **783 tests,
7 ignored**. All-target Clippy, default all-target compilation, formatting and
pin/EPL guards pass. All 42 original compatibility exports and their native case
records remain byte-identical, and the pinned case5 preparation fingerprint is
unchanged. PowerIO passes 217 affected tests and changed-library Clippy;
its full CI matrix retains the known unrelated float-comparison failures in
PSS/E/PMD/synthetic parity tests, with all C ABI variants and Python passing.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-fixed-mode-target \
  python3 evals/validation/acdc/physical/fixed-mode.py /tmp/tacdc-fixed-mode-live
CARGO_TARGET_DIR=/tmp/tacdc-fixed-mode-target \
  python3 evals/validation/acdc/physical/fixed-boundaries.py /tmp/tacdc-fixed-boundaries-live
```

The next development gate is native coupled PF with explicitly selected controls
and reference setpoints, starting with case5. Automatic mode reversal requires a
separate disjunction or justified physical boundary model. Generic imported-data
rebasing, physical pole/return wiring, droop/deadbands and LCC remain unqualified.
