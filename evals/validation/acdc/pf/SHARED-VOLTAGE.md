# Opt-in shared AC-voltage PF allocation

The new `PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1` profile solves
original case24 with its generator/converter voltage controls unchanged.
PowerIO is pinned to `dbc52de6650e43efa734125e398994960d8c2a02`; the independent
reference remains PMACDC 0.10.1 at `9ada4082ac087aed649f48fb5f977680b0fe06c7`
with the pinned Julia environment. Input is the **MATPOWER-extension variant**.
The natural JSON adapter remains separate.

## Selected reactive allocation

At each bus, all active generators and active `VoltageAtPcc` converters share
**equal increments in reactive injection** from their own declarations:

```
Qg_i - Qg_decl_i = delta
Qwithdrawal_c - Qwithdrawal_decl_c = -delta
```

One source bus Vm target serves the whole group. Generator Vg and converter Vtar
remain retained source declarations; neither overrides the bus target. Fixed-Q
converters do not participate. Equal increments are in common network power
units, not fractions of equipment ratings. This is an explicit allocation policy,
not a reconstruction of controller gains or a claim that all physical equipment
shares equally. Inactive participants leave the group but stay in source data.

The existing generator-to-generator sharing rows remain. A mixed group adds
`Qg_anchor + Qwithdrawal_c = Qg_decl_anchor + Qwithdrawal_decl_c` for each voltage
controlling converter. A converter-only group adds withdrawal-difference rows
relative to one identity-sorted anchor. Redundant Vm equations are omitted from
the native square system. Sorting chooses an algebraic anchor, not priority;
permuting input records preserves allocation and the electrical point.

Reference-bus active-power sharing, AC/DC reference coverage, Cinv electrical
losses, positive-voltage/current numerical safeguards and convergence tolerances
are unchanged. Limits are still reported after solving: no saturation, capacity
weighting, PV/PQ switching, automatic loss reversal or droop policy is introduced.

The original `PowerModelsAcDc0101AcpVscEqualIncrementV1` profile continues to
reject overlapping voltage authority. Its source/IR semantics and fingerprints
are unchanged. The new profile is explicit in IR and preparation fingerprints;
a start fingerprinted under the old profile is refused even where both profiles
produce identical equations and points.

## Qualification cases

| Case | What it checks |
| --- | --- |
| Original case24 | Unchanged source controls, including generator/converter voltage authority at 215 |
| Unchanged case5 | New profile agrees with the old profile when no authority overlaps |
| Mixed at PV | Two generators plus a converter at bus 1; unequal declared Q values |
| Mixed at reference | Generator/converter voltage group at the AC reference bus |
| Converter-only | Two converters regulate PQ bus 2; unequal declared withdrawals |
| Mixed multiple | Two generators plus two voltage-controlling converters at bus 1 |
| Inactive voltage controller | One converter in the mixed group is out of service |
| Tight ratings | Mixed group with Imax = 0.01 pu and generator Q bounds +/-1 MVAr |

Synthetic variants deliberately retain Vtar values 1.08 and 0.94 to verify that
the shared bus Vm, not those fields, controls the solve. Source controls and
settings round-trip through typed PF result IR. The original case24 source file
and in-memory declarations are unchanged.

The independent Julia model uses `PowerModelsACDC.build_acdcpf`, adding allocation
rows from the independently parsed generator Qg and converter Q_g declarations.
`Q_g` in that data is generation-positive, while `qconv_tf_fr` is withdrawal-positive;
the reference adapter explicitly converts that sign. PMACDC inserts duplicate Vm
rows for shared voltage controls. Only algebraically identical targets are
deduplicated for optimization, then every original row is restored for acceptance.
The existing exact affine/auxiliary presolve and numerical voltage-domain bounds
are also restored before validation. Native solved points are never used as
reference starts.

Each case uses declared, flat and deterministic perturbed starts, and each
solver's point is checked in the other model. The reference checks 96 existing
corruptions plus 27 targeted sharing corruptions. The targeted perturbations
preserve total bus reactive injection but break individual allocation: mixed
Qg/Qwithdrawal change in the same direction, converter-only withdrawals change
in opposite directions. Direct native injection-increment, source-retention,
profile refusal, inactive-coordinate, IR and stale-start checks pass. Three
mixed/converter-only derivative and input-order permutation cases pass.

[shared-voltage-qualification.json](shared-voltage-qualification.json) records the
profile, hashes, complete counts, references, control/allocation counts, restored
duplicate rows and numerical metrics. Full points, generated inputs and logs stay
outside the checkout. The original case24 points are additionally checked against
a separately built unmodified original-source PMACDC model, without allocation
rows; this establishes source-equation feasibility, not original-model uniqueness.

The shared profile passes **24 native + 24 reference solves**, **48 bidirectional
checks**, **123 corruption controls**, and **three original-source checks**.
Maximum native residual is **8.438e-11**; reference/cross residual is **8.087e-11**;
maximum electrical coordinate difference is **1.955e-10**. Original case24 takes
six native iterations from each start and satisfies original PMACDC equations
within **2.729e-13**. Tight ratings preserve the mixed group's electrical point
while increasing reported violations from one to nine.

The old case5 profile rerun passes its **48 solves, 48 cross-checks and 96
corruptions**, with all eight fingerprints unchanged. Full Tellegen suite:
**793 passed, 7 ignored**. All-target Clippy, default all-target compile,
formatting and pin/EPL guards pass. The 42 compatibility exports and their native
case records remain byte-identical.

## Compatibility and IR

Only PowerIO's unreleased schema adds the profile token. The full frozen
`e5a7c8b8` decoder accepts old PF IR and rejects the new token in both PF instances
and results. Current round trips and downgrade refusal pass. PowerIO passes 220
affected tests and changed-library Clippy. All seven CI Clippy targets were
attempted: C ABI/Python pass; existing PSS/E/PMD/synthetic-parity float-comparison
failures remain in workspace/no-SINCAL/matrix-gridfm.

No OPF behavior changes. The original compatibility exports/native records and
case5 preparation fingerprint remain unchanged. Existing draft PR heads remain
unchanged; work stays on the two development branches.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-shared-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-shared-pf-live --suite shared
CARGO_TARGET_DIR=/tmp/tacdc-shared-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-shared-pf-case5-recheck
```

The follow-up [case39 study](CASE39.md) explicitly selects both a DC-voltage
controller and fixed generator Pg dispatch; control-only failures are recorded. Original case67 still needs an AC-island PF reference/dispatch choice
and a separately defined droop policy. Physical-unit/fixed-mode PF, saturation,
generic rebasing, sparse performance and physical pole/return wiring remain
separate. No new PMACDC electrical-equation bug is established by this work.
