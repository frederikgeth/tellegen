# Optimized physical-base equivalence — 2026-11-10

The physical-coordinate profile now has independent **local OPF** qualification
across explicitly equivalent base representations. This follow-up changes no
production equations, profiles, schema or PowerIO pin (`ba1f9fc9`). It adds a
reproducible validation harness, native regressions and compact evidence on the
existing `codex/acdc-native-case5` branch.

## Fixture scope and physical bounds

There are 32 physical networks: all eight transformer/reactor/filter flag
combinations, both power directions and one/two symmetric poles. Each uses the
same phase-domain circuit construction as [the coordinate gate](COORDINATES.md),
with one AC generator/PCC, one converter and a two-node resistive DC branch.
Each physical network has four representations:

| Representation | MVA | PCC kV | Converter kV | DC from/to kV per pole |
| --- | ---: | ---: | ---: | ---: |
| Reference | 100 | 230 | 200 | 500 / 500 |
| MVA only | 50 | 230 | 200 | 500 / 500 |
| Voltage bases only | 100 | 400 | 345 | 250 / 1000 |
| Combined | 250 | 230 | 400 | 800 / 250 |

The builder explicitly converts R/X, susceptance, normalized transformer tap,
voltage starts/limits and current limit from fixed physical declarations. The
DC branch retains the declared from-side resistance convention. Bounds are
matched in physical units: PCC 180–300 kV, valve 180–240 kV, filter 150–288 kV,
each DC node 450–550 kV, valve current 0–0.3 kA, and unchanged MW/MVAr limits.
Every native variable bound is converted back and compared across all 128
representations; the independent Julia parser/model bounds must match too.
PF settings are retained declarations, not optimization controls.

The generator cost is `0.002*P_MW² + 3*P_MW + 1`, strictly increasing throughout
its -150–150 MW bounds. This makes loss reduction economically preferable in
both flow directions and avoids the preceding analytical fixture's stationary
cost point inside the negative-power range. No equipment or cost is changed
between the four representations of a given physical network.

Each representation is solved from two explicitly equivalent physical starts:
the analytical feasible point and a deterministic perturbation of its voltages,
powers, current and station angles. Neither solver receives the other's optimum.
Native regression checks require actual iterations and cost improvement from
the feasible analytical point; the deliberately infeasible perturbation is not
used as an objective lower bound.

## Independent reference and acceptance

The pinned Julia environment and PowerModelsACDC revision remain unchanged.
Fresh MATPOWER-extension exports are parsed by Julia. The checker restores
declared Imax and substitutes the documented physical kV/MW/ohm bypass and DC
rows. Enabled station equations, losses, auxiliary bounds and objective remain
PowerModelsACDC's. These are explicitly **modified-reference** comparisons.
The shared substitution helper also reproduces the prior 384-point report
exactly. Raw-pu voltage compatibility warnings from source preprocessing are
not interpreted as physical infeasibility; all actual model bounds are checked.

For each solved point the harness checks every native/reference constraint and
bound, all 22 native and 23 reference variables, objective evaluation and
successful local solver status. The unused VSC firing angle is accounted for
separately. Every solution is converted through physical kV/kA/MW/MVAr/radians
and evaluated on every equivalent base representation:

- 256 native and 256 Julia local solves over 128 network representations.
- 2,048 Julia cross-feasibility checks of both solvers' points across bases.
- 1,024 native cross-feasibility checks of Julia's points across bases, plus
  native-to-native checks during generation.
- 512 independent Julia corruption controls: +1 physical kV at the receiving DC
  node and +1 MW at the PCC, each applied to an otherwise feasible solved point.
- Direct phase-domain reconstruction of station series drops, filter current,
  terminal complex powers, RMS current, converter loss and DC Ohm/KCL equations
  at both solvers' physical points. This does not call production polar formulas.

The constraint threshold is `1e-7`, cost agreement uses `1e-6 + 1e-8*|cost|`,
and direct physical residuals use `1e-6` in their respective kV/kA/MW/MVAr units.
Reference substituted rows use SI units; native rows are per-unit, so their
raw residual norms are reported separately. Exact primal equality is not an
acceptance condition; physical differences are recorded without assuming
uniqueness. No tolerance was relaxed to obtain these results.

## Results

| Metric | Maximum |
| --- | ---: |
| Native own-model violation | 4.338e-11 |
| Native KKT error | 3.387e-9 |
| Julia own-model violation | 2.183e-11 |
| All-base Julia cross violation | 1.747e-10 |
| Julia solutions in native models | 5.834e-13 |
| Native–Julia objective difference | 4.795e-9 |
| Cost range across both starts/solvers/all bases | 4.797e-9 |
| Native direct physical residual | 1.722e-10 |
| Julia direct physical residual | 2.303e-11 |

All runs pass. The valve upper-voltage limit and one DC node's upper-voltage
limit bind; the chosen DC endpoint depends on flow direction. The declared
0.3 kA current bound is inactive in this suite; its binding/violating semantics
remain covered by the prior 108-case current gate. Physical solutions also
agree closely in this run: maximum voltage difference is about 2e-6 kV and
maximum active-power difference is about 1.9e-9 MW, without making that agreement
a uniqueness claim.

All **30 AC/DC regression tests** pass with `acopf mc-opf conic schema`, including
the four existing native corpus cases and the two new optimization tests.
All-target Clippy with warnings denied, formatting, pin and EPL guards pass.
The previous full-suite result remains 769 passed/7 ignored at 57fb3b90; this
validation-only follow-up reruns the affected suites rather than claiming a
new full-workspace result. Draft PR heads and the PowerIO branch are unchanged.

The [87,254-byte compact report](optimized-qualification.json) records all 32
groups, physical representatives, active bounds, ranges, source hashes and
complete-report hashes. Full inputs, points and logs are generated outside the
checkout. No new upstream discrepancy or bug was found.

## Reproduce and remaining gates

```sh
CARGO_TARGET_DIR=/tmp/tacdc-optimized-target \
  python3 evals/validation/acdc/physical/optimized.py /tmp/tacdc-optimized-live
```

The runner invalidates old result files, runs native generation, independent
Julia optimization, native verification and report aggregation in sequence.
It uses locked/offline Cargo and the pinned Julia environment. Remove only the
temporary Cargo target after validation; retain generated cases, logs and reports.

This qualifies local optimized solutions for the explicit single-converter
fixtures above. Multi-converter/meshed-network physical validation is the next
step. General imported-network rebasing still needs provenance and conversion
rules; arbitrary hardware projections, directional loss policy, physical
pole/return wiring, native coupled PF and global optimality remain separate.
