# Opt-in physical-coordinate PF

`PhysicalCoordinatesDcDroopSharedVoltageEqualIncrementV1` selects physical local
bases for PF, including DC-power droop. PowerIO is pinned to
`0ed1174cfb66975fedd0ad13af1eca5ee878354b`. The three existing source-pu profiles
retain their equations, allocation, admission rules and fingerprints. Selecting
an OPF physical profile and asking for PF still refuses: explicit PF intent is
required. Imported data is never silently rebased.

This qualifies the **MATPOWER-extension variant** of PowerModelsACDC. The
reference remains PMACDC 0.10.1 at `9ada4082ac087aed649f48fb5f977680b0fe06c7`,
with Julia 1.12.6. Natural JSON IR remains a separate future adapter.

## Physical control contract

Electrical coordinates follow [the physical-base contract](../physical/COORDINATES.md):
PCC voltage uses the AC bus base, filter/valve voltage uses the converter base,
and each DC voltage uses its own per-pole base. Current is AC RMS current in kA,
with `Ibase = Sbase / (sqrt(3) * Vconverter_base)`. Powers are total station MW
or MVAr. Explicit source declarations and units survive instance/result IR.

For withdrawal-positive DC converter power, a physical droop slope `R` in
per-pole kV per total station MW means:

```
Pdc_MW = Pref_MW - (Vdc_kV - Vref_kV) / R
k_pu   = R * Sbase_MVA / Vdc_base_kV
Vdcset_pu = Vref_kV / Vdc_base_kV
```

Under a base change, preserve physical `R`, `Pref`, and `Vref`:
`k_new = k_old * (S_new / S_old) * (Vbase_old / Vbase_new)`.
There is no extra pole-count factor: voltage is per pole and power is total.
Changing base metadata without converting impedances, targets, bounds, taps and
slopes describes different equipment/control; it is not a supported rebase.
The fixture converter is explicit and is not a general imported-network API.

Fixed DC-voltage targets use the controlled DC node's base. PCC reactive-power
control retains MVAr declarations. Shared AC-voltage participants retain the
existing equal reactive injection increments and common bus voltage target.
Positive finite droop slopes, finite reciprocals, positive targets, zero
deadband, island voltage authority and AC reference admission are unchanged.

The loss curve uses **Cinv in both directions**. The tests deliberately retain a
different unused Crec to detect accidental direction-based curve selection.
Explicit rectifier/inverter PF modes and automatic reversal remain separate.

PF limits are post-solve reports. Declared current limits, power bounds and
other original ratings do not clip the Newton equations or switch controls.
[Proper saturation](SATURATION.md) is planned separately, including loss of
voltage support when the last controller saturates.

## Independent validation

The synthetic circuit is derived in phase-domain kV, kA, ohms and siemens,
independently of Tellegen's polar per-unit tapes. Its known point is the oracle;
the two solve starts are declared data and a deterministic physical perturbation.
The latter is deliberately infeasible, not a supplied solved point.

The corpus has 64 physical/control cases and four base representations each:
all eight transformer/reactor/filter combinations, both transfer directions,
one/two poles, and fixed-voltage/droop control. AC bus, converter and DC endpoint
bases vary independently. Droop uses `R=0.2 kV/MW`, `Vref=498 kV`, and the solved
DC voltage is 500 kV, exposing wrong slope conversion even at the solution.

Reference models are fresh `build_acdcpf` models from exported MATPOWER-extension
files. They replace the previously qualified physical DC-line and transformer
bypass equations, plus the droop row expressed directly in MW/kV. This is an
explicitly modified PMACDC physical reference, **not equivalence to unmodified
source-pu PMACDC**. Exact single-variable targets/unused firing angles are
presolved and restored; SI electrical rows are normalized for solving and
checked in their original units. Reference starts are native initial points,
never native solved points. Cinv and declared current preprocessing are explicit.

[Compact evidence](physical-qualification.json) records source/harness/report
hashes, base choices and each physical group's extrema. Full native/reference
reports remain outside the checkout in `/tmp/tacdc-physical-pf-live`.

| Check | Result |
| --- | ---: |
| Native / independent reference solves | 512 / 512 |
| Native equivalent-base checks | 2,048 |
| Reference equivalent-base checks, both solvers' points | 4,096 |
| Reference points evaluated on native bases | 2,048 |
| Reference voltage/power/current corruptions rejected | 1,536 |
| Maximum native PF residual | 4.973e-11 |
| Maximum reference physical-row residual | 4.967e-9 |
| Maximum independent physical circuit/control residual, either solver | 7.703e-9 |
| Maximum physical coordinate difference across solvers/bases | 2.951e-8 |
| Native iteration range | 1–7 |

Rust tests additionally check eight physical shared-voltage allocations and
48 current-limit variants (below/at/above the
physical rating, with simultaneous active-power violations), six derivative
points, wrong droop/target/metadata conversions, missing sqrt(3) current scaling,
stale starts and result IR. Limit violations are converted back to kA/MW and
compared with the independent circuit, while the solved point remains unchanged.
The frozen PowerIO `70025e87` reader accepts the three old profiles and rejects
the physical token in instances/results. Only the unreleased schema changes.
[PowerIO validation](physical-powerio-validation.json) records 256 passing tests,
documentation checks, and the existing unrelated float-comparison lint failures.

The full Tellegen suite passes **805 tests, 7 ignored**. Final physical tests,
all-target numerical-feature Clippy, default all-target compilation, formatting,
pin agreement and EPL guard pass. The case5 rerun retains all eight fingerprints
and passes 48 solves/checks and 96 corruptions; all 42 compatibility exports and
their records are byte-identical. [Native validation](physical-native-validation.json)
and [case5 regression](physical-case5-regression.json) retain the evidence.

These are local roots of small converter/DC-line fixtures. They do not establish
uniqueness, global convergence, operational feasibility, or connected physical
PF outage/island performance. Connected physical PF and explicit fixed loss-mode
PF remain follow-ups; saturation, valve-power droop/deadbands and LCC remain
separate contracts. No new upstream PMACDC equation bug was established.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-physical-pf-target \
  python3 evals/validation/acdc/pf/physical.py /tmp/tacdc-physical-pf-live
CARGO_TARGET_DIR=/tmp/tacdc-physical-pf-target CARGO_INCREMENTAL=0 \
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p tellegen --features 'acopf mc-opf conic schema' --locked --offline
```

## Connected networks and fixed modes

The separately recorded [follow-up milestones](MILESTONES.md) now qualify connected
physical PF/outages and explicit fixed loss-mode PF. This original Cinv-only
profile and its recorded evidence remain unchanged. Saturation stays separate.
