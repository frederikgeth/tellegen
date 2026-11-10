# Canonical coupled VSC PF and OPF API

This page describes the current development implementation. PowerIO owns the
immutable source network, canonical intent, typed points/results, IR and OPF
numerical preparation. Tellegen owns expressions, derivatives, numerical starts,
solvers and validation diagnostics. These interfaces are not a published release.
Review dependencies and release planning are tracked in
[PowerIO #819](https://github.com/eigenergy/powerio/issues/819).

The input adapter is the **MATPOWER-extension variant of PowerModelsACDC**.
PowerModelsACDC's native JSON serialization is a separate future reader.

## Public surface and ownership

`PreparedAcDcOpf::new(AcDcOpfInstance)` and
`PreparedAcDcPf::new(AcDcPfInstance)` accept explicit canonical PowerIO intent.
Both are exposed through `tellegen::ac_dc` under the native `acopf` feature.
The legacy physical-terminal/reduced-converter API in that module is separate;
its supported models do not extend the full-station VSC profile scope.
Existing browser, CLI, server and Python adapters gain no new solve mode.

PowerIO's `build_ac_dc_opf_preparation` supplies the AC arrays and every active
DC/station block. A consumer must assemble all of them. There is no standalone
public PF matrix builder: Tellegen builds a private electrical projection and
adds PF control/allocation equations. Replacing costs in that private projection
does not change source declarations or make costs PF controls. Ordinary AC-only
constructors and preparation still refuse nonempty equivalent equipment.

Preparation rejects unsupported representations: normalized or mismatched-MVA
owners, mixed aggregate/equivalent DC, active AC self loops, LCC, storage and
three-winding transformers. OPF preparation qualifies polynomial generator
costs; piecewise costs need separate auxiliary-coordinate support. No profile
infers physical pole/return wiring or performs generic network rebasing.

## Explicit OPF profiles

| `AcDcOpfProfile` | Coordinates | Current policy | Quadratic valve loss |
| --- | --- | --- | --- |
| `PowerModelsAcDc0101AcpVscV1` | Source pu | Pinned reference Imax repair | Cinv in both directions |
| `DeclaredCurrentAcpVscV1` | Source pu | Declared Imax | Cinv in both directions |
| `PhysicalCoordinatesAcpVscV1` | Physical local bases | Declared Imax | Cinv in both directions |
| `FixedModePhysicalAcpVscV1` | Physical local bases | Declared Imax | Explicit station Crec/Cinv and valve-P sign |

All minimize generator cost with free generator/converter dispatch. Retained PF
controls and targets do not constrain OPF dispatch. Reference auxiliary bounds
remain part of the selected model; the physical selection does not imply a
complete physical hardware model. The compatibility profile is pinned to PMACDC
0.10.1 at `9ada4082ac087aed649f48fb5f977680b0fe06c7`, including Cinv selection and
Imax repair. Selecting declared current alone does not rebase source coordinates.

`AcDcAngleReferencePolicy::Declared` is the default. The opt-in
`LowestBusIdPerIsland` preserves declared references and adds a zero-angle
numerical anchor at the lowest source bus ID in each uncovered active AC island.
DC links do not synchronize AC phase. Source bus roles and PF generation/voltage
controls remain unchanged. `align_angle_references` explicitly shifts bus and
station internal angles together, records each shift, and rejects finite-bound
violations instead of clipping or wrapping. Stored starts are not rotated silently.

## Explicit PF profiles

| `AcDcPfProfile` | Added semantics |
| --- | --- |
| `PowerModelsAcDc0101AcpVscEqualIncrementV1` | Source-pu/Cinv; fixed PCC P or Vdc, fixed PCC Q or bus Vm; equal generator increments |
| `PowerModelsAcDc0101AcpVscSharedVoltageEqualIncrementV1` | Shared generator/converter voltage control with equal reactive injection increments |
| `PowerModelsAcDc0101AcpVscDcDroopSharedVoltageEqualIncrementV1` | DC-port power/voltage droop and island-local authority checks |
| `PhysicalCoordinatesDcDroopSharedVoltageEqualIncrementV1` | Physical local bases and declared ratings; Cinv in both directions |
| `FixedModePhysicalDcDroopSharedVoltageEqualIncrementV1` | Explicit station Crec/Cinv selection and final valve-P mode domain |

Every active AC island requires exactly one explicitly declared reference with
active generation. Numerical OPF anchors do not satisfy this requirement.
Non-reference generator Pg stays declared. Reference generators share changes
from declared Pg equally; generators at PV/reference buses share changes from
declared Qg equally. Effective bus roles follow the recorded reference repair;
source roles remain immutable. Bus Vm supplies the voltage target, not retained
generator Vg or converter Vtar.

Shared voltage groups allocate equal **reactive injection** increments:
`delta Qg = -delta Qpcc_withdrawal`. Fixed-Q converters do not participate.
There are no rating weights or saturation. The original profile still rejects
voltage overlap. The first two profiles require exactly one fixed-Vdc controller
per DC island. Droop-enabled profiles require at least one fixed-Vdc or positive
droop controller and at most one fixed-Vdc controller in each island.

DC-power droop uses withdrawal from the DC bus into the station:
`Pdc_pu - Pdcset_MW/Sbase + (Vdc_pu - Vdcset_pu)/k = 0`.
It requires a finite positive k with finite reciprocal, a positive target and
zero deadband. Type-4 valve-power droop is unsupported. On physical local bases,
a slope R in kV per pole per total MW has `k = R*Sbase/Vdcbase`; no additional
pole-count multiplier applies to total power.

PF operating ratings are checked after solving and reported as
`limit_violations`. They do not clip the point, switch controls or turn an
otherwise converged electrical PF into OPF dispatch. A selected fixed-mode
valve-P sign, in contrast, is part of the accepted root's model domain.
[Proper saturation](pf/SATURATION.md) remains a separate future control policy.

## Coordinates, signs and fixed modes

Canonical `AcDcOperatingPoint` values contain complete `AcDcQuantity`/identity
pairs. Powers are MW/MVAr, angles radians, voltages pu under the selected profile
and valve RMS current pu on the station AC base. Generator powers are production;
PCC and valve terminal powers are passive withdrawals. Valve P enters from the
reactor. DC station power is withdrawal from the DC bus. Missing, duplicate,
unknown and nonfinite point coordinates refuse. Row order is not identity.

Physical local coordinates use the PCC line-to-line bus base and the station's
converter base for filter/valve ports and impedances. An enabled transformer tap
includes `Vconverter/VPCC`; bypass equates physical voltages. DC branch resistance
uses its from-node per-pole voltage base, so orientation reversal also converts
resistance. The [coordinate specification](physical/COORDINATES.md) and
[bypass checks](physical/BYPASS.md) distinguish this from source-pu behavior.
Equivalent-base validation is not a generic rebase API.

Fixed-mode profiles require exactly one identity-keyed `AcDcStationLossMode` per
active station, sorted on output. Rectifier selects Crec and valve AC withdrawal
P >= 0; inverter selects Cinv and P <= 0. PCC/DC signs are not substitutes. At
zero P the selected curve remains in force, including at nonzero reactive
current. Zero current retains constant loss. Unknown, missing, duplicate or
inactive assignments and an empty mode-sign/rating intersection refuse.

Newton evaluates the smooth selected loss curve during iteration, then checks
the final mode domain. It does not automatically change modes. Ordinary valve-P
ratings remain post-solve PF reports. OPF enforces the selected sign and operating
bounds together. Automatic reversal needs a separate discrete/complementarity
policy and derivatives through that policy are not qualified here.

## Persistence, starts and solver acceptance

PF and OPF each have distinct PowerIO instance/result structural types, require
`balanced-acdc-equivalent-v1` and reject unknown fields/profiles. Only the
unreleased IR schema changes. PF's first four profiles omit `loss_modes` and
reject explicit arrays or null; fixed-mode PF requires a complete array. OPF's
older profiles reject nonempty modes and omit the empty list. MATPOWER export
preserves equipment and both source coefficients, not the chosen solver mode list.

Preparation fingerprints include a versioned formulation, the complete source
network and selected profile. OPF v3 includes angle policy and anchor records;
fixed-mode identities add station modes. Absent modes are omitted to preserve
older fingerprints. Source/profile/topology edits invalidate diagnostic starts.
Content identity is deliberately conservative, not an electrical equivalence test.

Both start enums provide `Declared`, `Flat`, `InstancePoint` and `Point`.
Declared/flat starts are numerical guesses. OPF constructed starts are projected
to bounds; explicit supplied starts remain exact. `InstancePoint` consumes the
complete canonical typed guess. `Point` requires a complete finite native name
map and matching preparation fingerprint. Explicitly supplying a typed guess
is not a claim that it came from the same immutable source model.

OPF uses POUNCE restoration/recovery and cancellation. Acceptance requires a
successful local status, finite point/objective, native and solver constraint
violations <=1e-7, KKT error <=1e-6 and objective agreement. Results include the
canonical `AcDcOpfSolution`, starts, diagnostics and recovery provenance. They do
not establish global optimality, qualified duals or infeasibility on failure.

PF uses damped Newton with analytic Jacobians and nonnegative current retraction,
residual tolerance 1e-10 and final native validation <=1e-9 including mode domain.
Original rating violations above 1e-7 are returned separately. Results include
`AcDcPfSolution`, initial/final points, iteration counts and equation checks.
Failure of a local trial does not prove absence of a root. PowerIO serialization
checks finite shape and semantics; producer status alone certifies neither
feasibility nor optimality.

## Qualification and its limits

Independent checks map every native coordinate into the pinned PMACDC model,
restore eliminated/duplicate equations for acceptance, and test deliberate
corruptions. Native values are not filled from a reference solved point. OPF
compares feasibility and objective, without demanding identical nonunique dispatch.
PF compares locally obtained roots and controls; it does not prove uniqueness.

- Source-coordinate OPF: case5, case24, case39 and case67, including explicit
  numerical anchors for case67; see the [evidence index](README.md).
- Source-coordinate PF: case5 and case24; [case39](pf/CASE39.md) requires an
  explicitly selected fixed-Pg dispatch and Vdc controller; [case67](pf/DROOP.md)
  explicitly promotes isolated bus 67 to a PF reference. Unedited originals
  remain refused where authority/reference requirements are missing.
- Physical OPF: local bases, bypass, mesh, connected AC/DC outages and fixed
  modes; see [connected](physical/CONNECTED.md) and [fixed modes](physical/FIXED-MODES.md).
- Physical PF: [base conversion](pf/PHYSICAL.md), [connected outages and fixed
  modes](pf/MILESTONES.md), including mixed station modes and negative controls.

Physical reference harnesses explicitly replace source-pu bypass/DC rows and
loss/droop policies where needed. They are modified PMACDC studies, with separate
phase-domain circuit checks, not equivalence claims against unmodified PMACDC.
No new upstream equation bug was established by the latest milestones.

LCC, explicit physical pole/return wiring, generic rebasing/projections, valve-power
droop/deadbands, saturation, automatic mode reversal, larger-case sparse/performance
work, natural JSON input and new binding/application solve modes remain separate.
