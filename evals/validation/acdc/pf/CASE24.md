# Case24 PF controls, static outages and islands

This extends the native PF qualification using the existing
`PowerModelsAcDc0101AcpVscEqualIncrementV1` contract. PowerIO remains pinned to
`e5a7c8b82845e32de7427f39ff8e63768f5f8bd2`; PMACDC remains 0.10.1 at
`9ada4082ac087aed649f48fb5f977680b0fe06c7`. No control, electrical profile, base
policy, operating limit or acceptance tolerance changes.

## The source case has overlapping voltage control

The MATPOWER-extension file `case24_7_jb.m` contains 50 AC buses, 65 generators,
77 AC branches, seven converters, three AC islands and two DC islands. Its name
does not mean that the complete AC/DC case contains only 24 AC buses.

The unmodified source is **rejected by the native PF profile**: converter 6 and
generators both regulate bus 215's voltage. The pinned reference adds duplicate
Vm setpoints and leaves the converter/generator reactive split undetermined.
This is a supported-input boundary, not a new upstream equation-bug claim.

Every positive case below makes one explicit baseline change: converter 6 uses
its retained PCC reactive-power withdrawal, +10 MVAr, instead of AC-voltage
control. The bus generators retain voltage control. The source file is unchanged;
only the labeled variant selects different control intent. The baseline variant's
three native points also satisfy the **unmodified original PMACDC equations**,
without added generator-sharing or converter-Q constraints. This establishes
source-equation feasibility of those particular points, not native support for
the original ambiguous control declarations or uniqueness of their solution.

## Qualified variants

| Variant | Change from the explicitly selected baseline |
| --- | --- |
| `declared_q_at_215` | Converter 6 uses its retained PCC Q setting |
| `pcc_active` | Converter 2 PCC P withdrawal increases by 10 MW |
| `pcc_reactive` | Converter 6 PCC Q withdrawal decreases by 7 MVAr |
| `dc_voltage` | Converter 1 Vdcset changes to 1.01 |
| `pcc_voltage` | Converter 2's AC target, bus 204 Vm, changes to 1.01 |
| `tight_ratings` | All converter Imax = 0.01 pu; generator Pmax = 1 MW, Q = +/-1 MVAr |
| `ac_line_outage` | AC branch 101--102 is out of service; islands stay connected |
| `ac_island` | Open AC connections to 107 and explicitly make its generator bus a reference |
| `dc_island` | Open DC branch 2--3 and explicitly assign converter 2 DC-voltage control |
| `converter_outage` | Converter 5 is out of service; DC bus 5 stays in the network |
| `combined_islands` | Apply both the AC and DC splits above |

Outages preserve inactive source records. Their solved coordinates are absent.
The AC split yields four AC references; the DC split yields three DC-voltage
controllers, one per DC component. DC connectivity never supplies an AC angle
reference. These are static snapshots with explicitly assigned controls, not
automatic islanding, controller switching or dynamic transition simulations.

Nine case24 refusal cases cover the original overlap, duplicate converter voltage
authority, missing/duplicate AC references, missing/duplicate DC controllers,
post-outage missing controls and a reference island without generation. The
post-outage preparation also rejects the old fingerprinted start.

## Near-idle Newton safeguard

The first native DC-split trials stalled from declared and flat starts. An
independent PMACDC solve found a feasible root with the isolated converter's
Pdc = 0 and valve current approximately 0.01368817 pu. The failure was native
Newton globalization near idle, not evidence of an infeasible network.

The squared current equality admits a negative-current algebraic root. A Newton
direction toward it can pin a nonnegative-current line search at zero. Solver
`analytic_jacobian_damped_newton_current_retraction_v2` first tries the ordinary
Newton candidate. If it is rejected, it tries the same voltage/power candidate
with each valve current set to `hypot(Pvalve,Qvalve)/Vvalve`. That exact equality
selects the nonnegative root. The full residual decrease and positive-voltage
checks still decide acceptance; no rating is enforced, clipped or relaxed.
Accepted current retractions are recorded separately from iterations. A zero
or ill-conditioned control Jacobian still fails explicitly, and failed trials
are not labeled infeasible. Regression tests require this safeguard in the
near-idle DC and combined split cases.

## Evidence and reproduction

[case24-qualification.json](case24-qualification.json) records the source and
harness hashes, active coordinate counts, references, controls, iterations,
current retractions, reported-rating violations and all refusal diagnostics.
Full points, exports and logs remain in the runner's external directory; their
hashes and representative-point digests keep the committed report compact.
[case5-recheck.json](case5-recheck.json) records the earlier case5 suite against
the new numerical solver. The original case5 qualification remains historical.

Each positive case is freshly exported and independently parsed by PMACDC.
Both solvers use declared, flat and perturbed starts. PMACDC adds only the
explicit generator-sharing policy and the previously documented numerical
presolve/domain guards, restored before validation. Each point is checked in
the other formulation. Generator-sharing corruption preserves total Q while
violating allocation; PCC P, PCC Q and DC-voltage corruption are also detected.
Generator allocation and all active converter setpoints have direct native
checks. Ratings remain post-solve observations, not convergence requirements.
Angles are compared modulo 2pi, with raw points and windings retained.

The 11 positive case24 variants pass **33 native + 33 independent reference
solves**, **66 bidirectional checks**, **132 corruption controls** and **three
unmodified-source point checks**. All native starts converge in six iterations.
Maximum native residual is **5.751e-11**; maximum reference/cross residual is
**9.596e-11**; maximum electrical coordinate difference is **2.105e-9**. The
unmodified original reference accepts the three baseline points with maximum
violation **2.729e-13**. Tight ratings leave the electrical point unchanged and
report 139 violations, compared with 29 for the baseline variant.

The case5 rerun passes **24 native + 24 reference solves**, **48 bidirectional
checks** and **96 corruption controls**. All eight PF fingerprints are unchanged;
maximum electrical difference remains **5.744e-10**.

Full Tellegen tests: **790 passed, 7 ignored**. All-target Clippy, default all-target
compile, formatting and pin/EPL guards pass. The existing case5 fingerprint and
42 compatibility exports/native records are unchanged. PowerIO code, pin and
schemas are unchanged; all existing PRs remain draft at their prior heads.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-case24-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-case24-pf-live --suite case24
CARGO_TARGET_DIR=/tmp/tacdc-case24-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-case5-pf-recheck
```

## Larger source cases and next decisions

The same profile screens the original case39 and case67 inputs before solving:

- **case39:** all ten converters declare PCC active-power control; no converter
  fixes DC voltage. The profile rejects the missing DC-voltage controller.
- **case67:** the AC component at bus 67 has no declared reference, so preparation
  rejects it. Seven converters also declare type-3 DC droop, which this profile
  does not support. The numerical OPF angle anchor does not supply PF dispatch
  or droop semantics.

Those two refusals are recorded alongside the nine case24 refusals. Earlier
PMACDC `LOCALLY_INFEASIBLE` outcomes remain failed local trials, not global
infeasibility proofs. They are not counted as native PF solves.

The next gate is an explicit control-policy decision: qualify shared
converter/generator voltage authority for original case24, select a labeled DC
reference variant for case39, and define supported droop/reference semantics for
case67. Do not silently repair these source declarations. Physical-unit and
fixed-mode PF, saturation, automatic reversal, generic rebasing, sparse/scaling
work and physical pole/return wiring remain separate.


The [new opt-in shared-voltage profile](SHARED-VOLTAGE.md) now qualifies original
case24 with its controls unchanged. The earlier profile and explicitly edited
variants documented here retain their semantics and refusal boundary.
