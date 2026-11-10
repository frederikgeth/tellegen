# Bypass and unequal-base qualification — 2026-11-10

All eight transformer/reactor/filter flag combinations have an independent
physical interpretation under the coordinate conditions below. Unequal-base
counterexamples reproduce limitations of the pinned source-coordinate equations.
This gate changes no production equations, profiles, schemas or dependency pins.
PowerIO remains at `896fef026feceb4cc2111a4914100fe313986386`.

| Equipment | Qualified physical interpretation | Unqualified conversion |
| --- | --- | --- |
| Enabled transformer | Converter-side ohms, normalized tap `T*V_converter_base/V_PCC_base`; unequal AC bases allowed | An arbitrary imported tap/impedance cannot be assigned these units without its source convention |
| Bypassed transformer | Ideal unity-ratio physical connection; common PCC/filter voltage base | Unequal local voltage bases; retained tap does not compensate |
| Enabled/bypassed reactor | Converter-coordinate ohms or ideal connection; filter/valve share the station voltage base | Independently based filter/valve ports are not represented by this owner |
| Enabled/disabled filter | Physical susceptance on converter base, or zero physical shunt | A disabled retained susceptance is not an active shunt |
| Resistive DC branch | One physical per-pole resistance with common endpoint voltage base and explicit pole multiplier | Interpreting each endpoint pu voltage on a different local base |

## Independent physical construction

The phase-domain circuit extends the preceding [physical gate](README.md).
Each disabled series element has zero **physical** impedance for the oracle;
a bypassed transformer has physical turns ratio one. A disabled filter has zero
physical susceptance. The source still retains nonzero R/X/B and a nonunit tap.
The oracle derives phase voltages/currents with Ohm's law and terminal powers
with `3 V conj(I)`, independent of production polar expressions.

The 384 cases comprise:

- **288 qualified cases:** eight flag combinations × two flow directions ×
  one/two poles × nine power/voltage-base combinations. Power bases are
  50/100/250 MVA. Enabled transformers use the prior unequal AC base triples;
  bypassed transformers use common PCC/converter bases of 200, 345 or 400 kV.
  DC endpoint bases remain common at 500, 250 or 800 kV respectively.
- **48 unequal-AC bypass witnesses:** four reactor/filter combinations with
  transformer bypass, both directions/pole counts and three power bases.
  PCC/converter bases are 230/400 kV; physical PCC and filter voltage are equal.
- **48 unequal-DC witnesses:** fully enabled/fully bypassed stations, both
  directions/pole counts, three power bases and receiving bases of 250/1000 kV
  against a sending base of 500 kV. Initial voltages, limits and points are
  consistently converted; branch resistance uses the sending-end base.

A 5 kA current limit keeps reference current repair inactive. Every case is
checked natively with both the compatibility and declared-current profiles.
Fresh MATPOWER-extension exports are parsed and instantiated in the unmodified
pinned PowerModelsACDC model. No source echo or solved reference point is used.

## Results and failure localization

All 288 qualified points satisfy equations and bounds within `1.137e-13`.
The 48 unequal-AC witnesses fail **only transformer voltage-magnitude bypass**
(residual 0.369565–0.371162 pu). The 48 unequal-DC witnesses fail **only the two
DC Ohm-law equations** (largest per-case violations 49.8281–2008.7367 pu).
Every other constraint/bound is within `1.137e-13`, and objective differences are
at most `1.137e-13`. These are expected physical-coordinate mismatches, not
accepted feasible points or optimizer infeasibility claims.

Julia identifies the expected constraints from their expression variables,
compares their residuals to native residuals, and rejects any additional failing
constraint. Every one of 22 native and 23 reference variables is accounted for.
All 384 sqrt(3) current-corruption controls fail in the previously satisfied
**current equation** (minimum violation 0.264576); an already failing bypass/DC
constraint cannot make that control pass by itself.

Native regressions also verify:

- Physical transformer/reactor/DC losses and cost, with unchanged source data.
- All 384 source IR round trips, including disabled fields and unequal bases.
- Disabled R/X/tap/B changes have no equation effect. Re-enabling each retained
  element produces a violation in its own equation family.
- Rescaling DC resistance using the receiving base cannot repair the missing
  endpoint-voltage ratio. Changing only voltage-base metadata leaves source pu
  equations unchanged but changes reconstructed physical volts.
- Generic normalization and AC-only OPF construction still refuse these owners.
- Dense derivative checks at two finite-difference steps pass for all eight
  station-flag combinations, including structural zeros and objective/Hessians.

Full numerical-feature Tellegen validation passes **766 tests, 7 ignored**;
all-target Clippy with warnings denied, formatting and pin/EPL guards pass.
The four existing native corpus regressions pass. All recorded fields in the
42 compatibility and 108 declared-current physical cases reproduce exactly.
PowerIO and the draft PR heads are unchanged.

## Why the unequal-base points fail

The pinned [transformer bypass](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/formconv/acp.jl)
imposes `v_PCC_pu = v_filter_pu`, without using the retained tap. An ideal
physical connection instead requires
`V_PCC_base*v_PCC_pu = V_converter_base*v_filter_pu`.
The equations coincide when those bases are equal. Adjusting an ignored tap
cannot supply the missing base ratio.

The pinned [DC equations](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/formdcgrid/acp.jl)
use `p_f = n/r * v_f*(v_f-v_t)` and the symmetric receiving equation.
If physical voltages have local bases `B_f` and `B_t`, and
`r = R*S/B_f²`, an explicit physical formulation would instead need
`alpha = B_t/B_f`:

```
p_f = n/r * v_f*(v_f - alpha*v_t)
p_t = n/r * alpha*v_t*(alpha*v_t - v_f)
```

No single resistance rescaling generally removes that missing ratio from both
ends. These formulas explain the witnesses; they are **not implemented** in the
source compatibility or declared-current profiles.

The source profiles continue to accept and evaluate declared source coordinates.
A base field alone is not proof of a permitted physical projection, and these
validation failures do not introduce a new runtime rejection of valid source
cases. The follow-up [physical-coordinate profile](COORDINATES.md) implements an
explicit local port/impedance-base contract and weighted bypass/DC equations
under a separate selection. General physical rebasing remains unavailable. Native coupled PF,
optimizer invariance, physical pole/return wiring and directional losses remain
separate work. No new upstream bug report was filed.

## Reproduce

```sh
CARGO_TARGET_DIR=/tmp/tacdc-bypass-target \
  python3 evals/validation/acdc/physical/run.py /tmp/tacdc-bypass-live --suite bypass
```

This uses the parent's pinned Julia environment and locked/offline Cargo
packages. Full generated cases, per-case results and logs remain in the output
directory. The checked-in [compact qualification](bypass-qualification.json)
contains all 14 coverage groups, representative cases, ranges and input/full-report
hashes (28,696 bytes); the 384 generated cases are not vendored. All are synthetic.
Remove the temporary Cargo target after validation, retaining evidence and logs.
