# Multi-converter physical DC mesh — 2026-11-10

This gate extends the [single-converter optimization checks](OPTIMIZED.md) to
three AC islands coupled through three VSC stations and a triangular DC grid.
It changes no production model, profile, schema or PowerIO pin (`ba1f9fc9`).
It remains on `codex/acdc-native-case5`; existing PR heads remain draft.

## Physical fixtures

Each AC island has one reference bus, generator and fixed load. The DC triangle
connects nodes 21→22→23→21 with per-pole resistances 10, 15 and 20 ohms. Converter
31/32/33 connects the corresponding AC bus 11/12/13 to DC node 21/22/23. There
are no AC branches, and DC links do not synchronize the three AC phase angles.
All three declared angle references remain explicit.

Twenty-four physical networks combine:

- Two station configurations: transformer/reactor/filter bit masks `[7,3,0]`
  or `[0,4,7]`, including unequal-base ideal bypass and disabled retained data.
- Cheap generation at island 0 or island 2, reversing economic transfer direction.
- One or two symmetric poles, with the same per-pole resistances.
- Unconstrained transfer, a 20 MW DC line rating, or a 0.18 kA cheap-station valve
  current limit. Other DC lines have 200 MW ratings and current limits are 0.5 kA.

The cheap island consumes 10 MW; each other island consumes 65 MW. Every island
consumes 10 MVAr. Generators have 0–200 MW limits and increasing physical costs
`0.002*P² + c*P + 1`, with `c=1` at the cheap island and `c=4+i` elsewhere.
DC lines connect converters without a fixed DC load. Congestion therefore forces
redispatch between AC generators, rather than merely changing losses supplying
one fixed withdrawal. Both terminal line ratings are enforced.

Each physical network is represented on four coordinate sets:

| Representation | MVA | PCC kV (three islands) | Converter kV | DC kV per pole |
| --- | ---: | --- | --- | --- |
| Reference | 100 | 230, 230, 230 | 200, 200, 200 | 500, 500, 500 |
| MVA only | 50 | 230, 230, 230 | 200, 200, 200 | 500, 500, 500 |
| Voltage bases only | 100 | 400, 230, 345 | 345, 400, 200 | 250, 1000, 800 |
| Combined | 250 | 230, 400, 200 | 400, 200, 345 | 800, 250, 500 |

The builder converts impedances, susceptances, taps, voltage/current limits and
starts from physical declarations. Every physical variable bound and both
initial points are identical across representations. The starts are a zero-flow
physical point and a deterministic perturbation; neither is a solved oracle,
and they need not initially satisfy converter losses. PF settings are retained
source declarations, not imposed OPF controls.

## Reference, identity and physical checks

Julia parses fresh MATPOWER-extension exports, restores declared Imax for every
station, and substitutes the documented SI transformer-bypass and DC Ohm-law
rows. This is an explicitly modified PowerModelsACDC reference. Every parsed
station/branch endpoint and generator association is checked. All 63 native
variables and 75 reference variables are accounted for; the additional 12 are
three unused VSC firing angles and nine fixed-load/zero-curtailment auxiliaries,
reconstructed from declarations without using solved oracle values.

Every native/Julia solution is evaluated on every equivalent base, with cost
agreement and all constraint/bound checks. Direct phase-domain reconstruction
checks each station's voltages, complex currents and terminal powers, converter
losses, per-pole DC Ohm laws, each DC node's KCL, AC island balance and total
network power/loss balance. The line/current constrained cases must actually
reach their intended limits and cost more than the unconstrained scenario.

Native regressions also reorder AC buses, generators, loads, DC buses, stations
and branches while retaining identities, then verify the same physical solution.
Six dense derivative cases cover both station configurations and all three
operating regimes. Corruption checks include +1 DC kV, +1 PCC MW and exchanging
two station DC injections. The exchange preserves total station injection but
must fail both affected nodal balances, detecting incorrect terminal association.

## Reference solver qualification and numerical finding

The initial default Ipopt settings returned a point with a `5.449e-7` SI DC
residual, exceeding the unchanged `1e-7` acceptance threshold. Tighter tolerances,
adaptive barrier and disabling automatic scaling alone did not reliably solve
both starts: numerical failures included mesh_067, mesh_017 and mesh_028.
Removing only redundant curtailment lower bounds also did not resolve the issue.
These failed trials are not counted as successful validation or infeasibility
proofs. Diagnostic logs are retained outside the checkout.

The final checker applies the same mathematically equivalent preprocessing to
**every** reference solve:

1. Construct the independent SI equations, then divide only the substituted DC
   rows by owner MVA and bypass rows by converter kV for numerical optimization.
   Every feasibility report multiplies their residuals back to the original SI
   units before applying the unchanged threshold.
2. Presolve the 12 reference-only auxiliaries. For each fixed load value, verify
   its source single-variable equality implies exactly the declared constant.
   Verify the VSC firing angle appears in neither objective nor any equation.
   Temporarily fix those variables and remove only their proven redundant
   equalities/bounds. After solving, restore all original auxiliary rows and
   bounds before checking any accepted point, including all cross-base checks.
3. Use uniform Ipopt settings: monotone barrier, no automatic NLP scaling,
   `tol=1e-9`, `constr_viol_tol=1e-9`, `acceptable_constr_viol_tol=1e-9`,
   `acceptable_iter=0`, exact derivatives, MUMPS and no bound relaxation.
   Native solver settings remain unchanged.

The pinned PowerModelsACDC fixed-load formulation imposes both `pcurt == 0`
and `pcurt >= 0`; the latter is redundant, and the VSC firing angle is unused.
That is a concrete numerical-conditioning concern, but these experiments do
not isolate it as the sole cause or establish a bug in unmodified PowerModelsACDC.
The modified SI reference needed both row normalization and exact auxiliary
presolve for this strict qualification. No electrical-equation discrepancy was
found, no operating limit was relaxed, and no upstream bug report was filed.
The report identifies all modifications and records the final solver options.

## Results and reproduction

All **192 native and 192 independent Julia local solves** pass, over 96
representations of 24 physical networks. The 1,536 Julia checks of both solvers'
points and 768 native checks of Julia points give **2,304 cross-formulation/base
checks**, in addition to 768 native-to-native base checks. All 576 Julia
corruption controls pass. Line/current cases reach their intended limits and
increase cost by 149.66–288.44 relative to the corresponding unconstrained case.

| Metric | Maximum |
| --- | ---: |
| Native own-model violation | 3.006e-10 |
| Native KKT error | 4.059e-9 |
| Julia own-model violation, original units | 2.274e-11 |
| Julia cross violation, original units | 3.007e-10 |
| Julia points in native models | 3.756e-13 |
| Native–Julia objective difference | 1.459e-8 |
| Cost range across both starts/solvers/all bases | 1.459e-8 |
| Native direct physical residual | 3.641e-9 |
| Julia direct physical residual | 2.372e-11 |

Constraint acceptance stays `1e-7`; cost agreement stays
`1e-6 + 1e-8*|cost|`; direct physical residuals stay `1e-6` in their respective
kV/kA/MW/MVAr units. Primal equality and global optimality are not required or
claimed. The [68,924-byte compact report](mesh-qualification.json) records every
group, representative physical dispatch, binding bounds, physical differences,
economic effect and complete-input/report hashes. Full solved points stay in
the generated evidence directory.

All **32 affected AC/DC regression tests** pass with `acopf mc-opf conic schema`,
including two mesh tests, six derivative cases and the existing four-case native
corpus. All-target Clippy, formatting, pin and EPL guards pass. The previous
769-test full-suite result remains historical; this validation-only follow-up
reruns the affected suites. PowerIO and existing draft PR heads are unchanged.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-mesh-target \
  python3 evals/validation/acdc/physical/mesh.py /tmp/tacdc-mesh-live
```

The runner invalidates stale reports and runs generation, Julia solves, native
verification and compact aggregation in sequence. Full synthetic files, solved
points and logs stay outside the checkout. Remove the temporary Cargo target
after validation, retaining evidence.

This gate covers local OPF on three otherwise separate AC islands with a meshed
DC grid. It does not qualify connected-AC-network interactions, outages or
islanding, global optimality, unique dispatch, general imported-network rebasing,
directional loss selection, explicit pole/return wiring or native coupled PF.
Connected AC/DC networks and outage/islanding behavior remain subsequent gates.

The optimization and mesh examples explicitly require `acopf`, fixing the
previous optimization example's missing feature gate. Default-feature
`cargo check -p tellegen --all-targets --locked --offline` also passes.
