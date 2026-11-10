# Case67 PF reference and DC-power-droop contract

This gate selects and checks the case67 PF contract before adding a native droop
PF profile. It provides **reference PF solves and native residual checks**, not
native droop PF solves. The input is the PowerModelsACDC **MATPOWER-extension
variant**. The natural JSON adapter remains separate.

PowerIO remains `dbc52de6650e43efa734125e398994960d8c2a02`; PMACDC 0.10.1 remains
`9ada4082ac087aed649f48fb5f977680b0fe06c7`, with the pinned Julia environment.
Original case67 is unchanged, SHA-256
`1f7637a093a5c9968f5571b323875a57f60f319a665600e0b5e11d9463c9cbf1`.
Its existing [source attribution and license](../../case67/README.md) apply to
freshly exported study variants; no additional upstream data is vendored.

## Explicit AC reference and dispatch choice

Original case67 has two AC islands: buses 1–66, with reference bus 1, and isolated
bus 67, whose source role is PV. Its generator declares Pg = 800 MW; converter 9
withdraws 800 MW at the PCC and regulates AC voltage. The baseline study changes
**only bus 67's role from PV to reference**. It keeps every generator Pg/Qg,
converter control/setting, equipment parameter and source voltage declaration.
A direct source-retention test reverses that role edit and compares the complete
network with the original.

The selected PF policy fixes bus 67's angle to zero and lets its generator Pg
balance that island. This is a dispatch/reference choice, not the OPF-only
numerical angle-anchor policy. The baseline solves at Pg67 = 800 MW. Raising
converter 9's PCC withdrawal by 10 MW makes Pg67 = 810 MW without changing its
retained declaration; the resulting generator rating violation remains visible.
All other nonreference generator Pg values stay fixed to their source values.
No OPF dispatch or solved state is used as a PF input or start.

At bus 67 the existing shared-voltage allocation contract supplies one common
bus-Vm target and equal reactive **injection** increments:

```
Qg67 - Qg67_decl = -(Qpcc9_withdrawal - Qpcc9_decl)
```

The source bus Vm is 1 pu; generator Vg and converter Vtar remain retained
settings and do not replace that target. Source Q declarations are both zero,
so the isolated bus's reactive balance and sharing row give Qg67 = Qpcc9 = 0.
The reference builder inserts duplicate voltage targets. The harness removes
only identical duplicates for optimization and restores them for acceptance.

## DC-power-droop equation and units

For selected type-3 converters, use the **DC-port withdrawal**, not PCC generation
or valve-side power:

```
r = Pdc_pu - Pdcset_MW / Sbase_MVA + (Vdc_pu - Vdcset_pu) / k = 0
```

Here `k = droop` is pu voltage per pu power. PMACDC's preprocessing divides
Pdcset by the case power base and leaves k unchanged. Increasing Vdc decreases
DC withdrawal: dPdc_pu/dVdc_pu = -1/k. At the source base of 100 MVA and k = 0.005,
a +0.001 pu voltage change corresponds to -20 MW withdrawal at fixed setpoint.
DC withdrawal has the opposite sign to injection into the DC grid. PCC `P_g` has
its own generation-positive source convention and is not the droop target.

Each of the seven source droop converters contributes one control row, replacing
a fixed-PCC-P row. Converter 1 retains its fixed Vdc controller and converter 9
retains fixed PCC P. A separate variant changes converter 1 to DC-power droop,
using its retained Pdcset/Vdcset/k, and solves with eight droop controllers and
**no fixed DC voltage**. A droop grid therefore must not be rejected solely for
lacking a fixed-Vdc controller. This example does not establish global uniqueness
or guarantee convergence for arbitrary droop parameters.

The selected first native extension should be opt-in and retain the existing
shared-voltage allocation. Its admission contract should require finite positive
k and positive Vdcset for active type-3 controllers, zero deadband, at most one
fixed-Vdc controller per connected DC island, and at least one fixed-Vdc or
positive-droop controller per island. It must retain all inactive/unused settings.
Type-4 valve-AC-power droop, negative/zero slopes, deadbands, saturation and
control switching need separate contracts. Generic power/voltage rebasing is
also separate; this gate does not authorize treating k as base-independent.

These rules are the next implementation contract. **No new profile token or
native admission rule is enabled by this gate.** Both existing native PF profiles
still refuse original case67 for missing AC reference coverage and the role-edited
variant for unqualified droop. Tests preserve those refusals.

## Executable study

| Variant | Change relative to the source-dispatch baseline |
| --- | --- |
| Reference 67 | Bus 67 explicitly becomes reference; all source controls retained |
| Droop power target | Converter 2 Pdcset increases by 25 MW |
| Droop voltage target | Converter 2 Vdcset increases by 0.002 pu |
| Droop coefficient | Converter 2 k doubles from 0.005 to 0.01 |
| Droop-only DC grid | Converter 1 selects type 3 using its retained settings |
| DC-line outage | Disable DC branch 1 between DC buses 1 and 2 |
| PCC active power at 67 | Converter 9 withdrawal increases by 10 MW |
| Tight ratings | Imax = 0.01 pu, generator P 0–1 MW and Q +/-1 MVAr |

PowerIO exports each variant freshly. PMACDC parses it independently and solves
from declared, flat and deterministic perturbed starts. Starts are generated from
source declarations and simple numerical constants; no solved point is reused.
The reference restores exact affine/auxiliary presolve and numerical voltage
bounds before checking the full original equations. The same 1e-7 residual and
electrical-coordinate acceptance thresholds apply; phases compare modulo 2*pi.

The native checker evaluates every successful point against the existing full
AC/DC electrical model, excluding OPF inequalities from PF equation acceptance.
It checks the selected PF controls independently in Rust: nonreference Pg,
reference angles, voltage targets, PCC P/Q, shared Q and DC-power droop. Original
ratings remain a separate report. This does not exercise a native droop Newton
system or its Jacobian; those are requirements for the next gate.

For each active droop row, the Julia audit checks its exact two-variable affine
form, power base, sign and coefficients, then compares a separate scalar equation
at three voltages on both sides of Vdcset and finite-differences both partials.
Power and voltage corruptions must fail the row in both checkers. A reference
sharing corruption preserves bus reactive balance but breaks equal allocation.

## Results

**23 of 24 reference trials solve**, and all 23 points pass native electrical and
control checks. This includes all starts for the baseline and droop-only grid.
The outage's declared start reaches `ITERATION_LIMIT` with residual 4.380; its
flat and perturbed starts solve. The failed iterate is retained outside the
checkout and is not counted as a solved point or proof of infeasibility.

Maximum reference/native electrical residual is **1.037e-12**; native control
residual is **3.730e-14**. Maximum electrical difference between successful starts
is **7.938e-12**. **492 scalar droop witnesses, 351 reference corruptions and 328
native corruptions** pass. The baseline has **13 rating violations**, the outage
46, and the tight-rating variant 49. Tight ratings preserve the electrical point.
These results do not establish operational feasibility.

[qualification.json](qualification.json) records source/harness/report hashes,
solver options, topology, row contracts, trial statuses and residuals. Full inputs,
starts, points, failed iterates and logs stay in the external output directory.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-case67-contract-target \
  python3 evals/validation/acdc/pf/case67/run.py /tmp/tacdc-case67-contract-live
```

Full Tellegen suite: **797 passed, 7 ignored**. All-target Clippy, default
all-target compile, formatting and pin/EPL guards pass. Existing profiles,
PowerIO pin and draft PR heads are unchanged.

Next: implement the explicit native DC-power-droop PF profile and its IR rejection
contract, prepare the droop rows, check derivatives and control/island refusals,
then run native declared/flat/perturbed solves against this reference study.
Keep the prior profiles unchanged. No new PMACDC equation bug is established.
