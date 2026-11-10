# Physical-unit and base-conversion qualification

The analytical circuit and fresh-export reference check qualify an explicitly
constructed physical interpretation of the full VSC station equations. They
also expose a voltage-base dependence in PowerModelsACDC's current-limit repair.
They do **not** enable generic normalization/rebasing or reinterpret arbitrary
imported source coordinates as physical equipment.

## Independent circuit and bases

The oracle in `crates/tellegen/tests/support/acdc_physical.rs` uses complex
phase-to-neutral RMS kV, per-phase RMS kA, ohms and siemens. It constructs
voltages from Ohm's law and powers from `3 V conj(I)`; it never evaluates the
production polar/per-unit branch equations to obtain expected powers.

- Valve line-to-line voltage: 200 kV; current: `(±0.25 - j0.08)` kA.
- Reactor: `0.7 + j4` ohms; filter shunt: `j0.0001` S.
- Transformer, referred to its converter side: `1.2 + j8` ohms; physical
  PCC/converter turns ratio: `T = 230/200`.
- DC line: 10 ohms per symmetric pole; sending voltage: 500 kV per pole.
  `P_from = n V_from I`, `V_to = V_from - RI`, `P_to = -n V_to I`.
- Aggregate valve loss: `0.4 + 0.7 |I| + 1.3 |I|²` MW. Coefficients retain
  their physical values; this test uses equal rectifier/inverter coefficients.
- Generator cost: `0.02 P_MW² + 3 P_MW + 1`.

For each direction and pole count (1/2), represent the same physical circuit
on all nine combinations of `S = 50,100,250 MVA` and voltage-base triples
`(V_PCC,V_converter,V_DC) = (230,200,500), (400,345,250), (230,400,800) kV`.
All angles receive one common rotation so the PCC angle is zero.

The conversions are explicit:

| Quantity | Conversion to the selected source coordinates |
| --- | --- |
| Power | `P_pu = P_MW/S`, `Q_pu = Q_MVAr/S` |
| AC voltage | Line-to-line magnitude divided by its local voltage base |
| Valve current | `I_pu = I_kA / (S/(sqrt(3)*V_converter))` |
| Transformer/reactor series impedance | `Z_pu = Z_ohm*S/V_converter²` |
| Filter susceptance | `B_pu = B_S*V_converter²/S` |
| Transformer normalized tap | `t_pu = T*V_converter/V_PCC` |
| DC voltage/resistance | `v_pu = V_kV/V_DC`, `r_pu = R_ohm*S/V_DC²` |
| Loss coefficients in pu current/power | `a=A/S`, `b=B/(sqrt(3)*V_converter)`, `c=C*S/(3*V_converter²)` |

Physical voltage/current/power limits and setpoints are converted too. Both DC
endpoints use the same DC voltage base. The pole multiplier affects DC power
and total loss, not the per-pole voltage drop or resistance. Canonical operating
points return MW/MVAr, pu magnitudes and radians; reconstructing kV/kA still
requires their declared bases.

## Checks and recorded result — 2026-11-10

`cargo test -p tellegen --features acopf --test acdc_physical` runs three tests:

1. All 36 representations satisfy native PF/OPF equations; OPF bounds, cost,
   physical transformer/reactor/DC losses and canonical result conversions agree.
   The 5 kA declared valve limit keeps reference current repair inactive.
2. Six further representations isolate the active current-repair limitation.
3. Seven corruptions detect wrong tap, transformer base, filter scaling,
   loss/current base, pole multiplier, DC resistance scaling and sqrt(3) current
   scaling. AC-only PF/OPF, generic normalization and mismatched owner/network
   power bases refuse; source declarations remain unchanged.

The live runner freshly emits all 42 circuits using PowerIO's **MATPOWER-extension
variant of PowerModelsACDC**. The pinned Julia environment parses, preprocesses
and instantiates each ACP OPF independently. Every one of 22 native coordinates
and 23 reference variables is accounted for; the sole reference-only value is
an unused VSC firing angle. Topology/identity is checked from the parsed source.
No solver or reference solution fills the analytical point.

[summary.json](summary.json) records source hashes, dependency/environment pins,
all bases, declared/effective currents and violations. The complete model checks
find **39 feasible points and three expected current-bound failures**. All other
constraints/bounds have maximum violation `1.137e-13`; objective differences are
zero in the recorded run. All 42 independent sqrt(3) corruption controls fail,
with minimum violation `0.264576`. The acceptance tolerance is `1e-9`; no tolerances
were relaxed for the current-limit witness.

## Confirmed upstream base-invariance limitation

PowerModelsACDC 0.10.1's
[`_check_conv_parameters`](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/core/data.jl)
raises `Imax_pu` to at least `hypot(Pacrated_pu,Qacrated_pu)`. That is a
nominal-1-pu voltage assumption. With fixed physical P/Q ratings, converting
the adjusted limit back to kA gives
`hypot(Pacrated_MW,Qacrated_MVAr)/(sqrt(3)*V_converter_base_kV)`.
It is independent of the MVA base but depends on the chosen voltage base.

The witness has ±100 MW/MVAr ratings, declared limit 0.05 kA and physical current
`0.262488095 kA`. Merely changing the converter base from 200 to 400 kV (with
all impedances, taps, voltages, bounds and declared current consistently
converted) changes the repaired limit from `0.408248290` to `0.204124145 kA`.
At every tested MVA base the first representation accepts the point and the
second rejects **only the current upper bound**. Native Tellegen and pinned
PowerModelsACDC agree on both outcomes; the equations and physical cost agree.
This is a reproducible upstream limitation/bug candidate for arbitrary
voltage-base conversion, not evidence that the reference equations are wrong.

The compatibility profile deliberately retains that preprocessing behavior and
records declared/effective limits. A physically invariant limit policy needs a
separately selected profile and an explicit physical/nominal voltage contract;
changing the existing profile silently would break benchmark equivalence.
No upstream issue was filed. The original physical-circuit gate changed no
runtime equations or pins; the declared-current follow-up below adds the
separately selected policy.

## Reproduce and scope

Use the pinned Julia 1.12.6 environment in the parent directory, with packages
already instantiated, and a separate temporary output directory:

```sh
CARGO_TARGET_DIR=/tmp/tacdc-physical-target \
  python3 evals/validation/acdc/physical/run.py /tmp/tacdc-physical-live
```

The runner uses locked/offline Cargo dependencies, disables incremental/debug
build artifacts by default, and invalidates a previous success summary before
running. Output files can be regenerated in place. Retain logs/results, then
remove the temporary Cargo target when finished. Generated `.m` cases are
synthetic, not copied PGLib fixtures; only the compact result is checked in.

This gate covers full enabled transformer/filter/reactor circuits, common DC
endpoint bases, equal directional loss coefficients, scalar symmetric poles
and analytic feasible points. It does not qualify bypassed transformers with
unequal bases, unequal DC endpoint bases, physical pole/return topology,
direction-dependent loss selection, arbitrary imported coordinate projections,
native coupled PF solves, derivatives under a new physical profile, or OPF
optimum invariance. Existing source-profile solve/derivative evidence remains
in the parent corpus. Natural PowerModelsACDC JSON IR is still a future adapter.


## Declared-current profile — 2026-11-10

Select `AcDcOpfProfile::DeclaredCurrentAcpVscV1` explicitly to honour the
source valve-current limit. The stored token is `declared_current_acp_vsc_v1`.
PowerIO revision `896fef026feceb4cc2111a4914100fe313986386` derives
`AcDcCurrentLimitPolicy::Declared` from that profile and records it beside
station declarations/effective bounds. The compatibility profile remains
`PowerModelsAcDc0101AcpVscV1`; its repair cannot be overridden beneath that token.

The new selection changes only the current upper bound. Equations, inverter
loss selection, P/Q ratings and auxiliary bounds are retained. Limits use the
station AC RMS current base `S/(sqrt(3)*V_converter)` kA. A low or zero declared
limit remains low or zero even if the P/Q rating box is much larger; preparation
never repairs a potentially infeasible model. This is a qualified current-bound
policy, not a complete new physical equipment formulation.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-current-target \
  python3 evals/validation/acdc/physical/run.py /tmp/tacdc-declared-current-live \
  --profile declared-current
```

Each of the 36 physical representations is checked with limits of 0.05 kA,
exactly the analytical current magnitude, and 0.3 kA. Thus **108 cases** comprise
36 strictly feasible, 36 binding and 36 violating current bounds. The declared
and effective physical limits agree across all nine bases. The violating cases
exceed the limit by `0.212488094968134 kA`, independent of representation.

The independent Julia check restores **only the source Imax** after ordinary
reference preprocessing and before model instantiation. It records both the
reference-repaired and selected limits. This explicitly modified reference is
not called unmodified PowerModelsACDC equivalence. Source voltage/power bases,
identities and all 22 native/23 reference coordinates are checked. All other
constraints/bounds are within `1.137e-13`; objective differences are zero. All
108 sqrt(3) current-corruption controls fail. Full pinned results and hashes are
in [declared-current.json](declared-current.json).

Native regressions also compare every equation and bound between profiles,
allowing only the current upper bound to differ. Typed operating-point/IR
round trips retain feasible **and infeasible** supplied points and their policy;
they never clip a point to make it pass. Different profiles have different model
fingerprints and reject each other's diagnostic starts. The profile is OPF-only:
PF diagnostics omit operating limits and therefore refuse this selection.
PowerIO additionally tests zero current limits, nested solution persistence,
unknown profile/override refusal and the old AC-only/normalization guards.
A frozen 552fc413 profile decoder accepts the old token and rejects the new one.

Compatibility checks with the new pin reproduce every field and source hash of
the previous 42 circuit records. The fresh case5 full pipeline passes and its
fingerprint remains `sha256:7c58f42f6628cd17c86fee45f95dea7b0366ec555b29eb3f39b04542660b3912`.
The full Tellegen suite passes 762 tests (7 ignored), including case5/24/39/67
native solve/derivative regressions; all-target Clippy, pin/EPL/fmt guards pass.
PowerIO's affected suites pass 493 tests; its previously documented broad Clippy
baseline warnings remain. No native optimum-invariance or new PF-solve claim is
made. The subsequent [bypass qualification](BYPASS.md) records which coordinate
conditions pass and which unequal-base interpretations remain unsupported.


## Bypass and unequal endpoint bases

The [384-case follow-up](BYPASS.md) qualifies all eight station-flag combinations
under explicit common-base conditions and isolates two unequal-base limitations
in both native profiles and unmodified Julia. See its supported-interpretation
table before using source per-unit declarations as physical equipment.

## Physical local coordinates — 2026-11-10

The separate `PhysicalCoordinatesAcpVscV1` profile now covers unequal-base ideal
transformer bypass and DC branches with explicit from-side resistance bases.
It retains declared current bounds. See [the contract, independent SI checks
and reproduction commands](COORDINATES.md). Existing source profiles and their
historical evidence below/above remain unchanged; general rebasing stays out
of scope.

## Optimized physical-base equivalence

The [local optimization gate](OPTIMIZED.md) checks 128 explicitly equivalent
network representations with 256 native and 256 modified-reference Julia
solves. Both solvers' points pass cross-feasibility, physical bounds and cost
checks on every equivalent base. This covers single-converter synthetic
networks; it does not enable general rebasing or qualify global optimality.
