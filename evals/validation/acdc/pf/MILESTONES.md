# Connected physical PF and explicit fixed loss modes

The two remaining physical PF milestones are implemented on
`codex/acdc-native-case5`, with PowerIO pinned to
`3e41473984585c0b38560f47ba3e0c2d930b029b` on `codex/819-acdc-models`.
The existing four PF profiles retain their wire encodings and semantics. The
reference is PMACDC 0.10.1 at `9ada4082ac087aed649f48fb5f977680b0fe06c7`,
using Julia 1.12.6 and the **MATPOWER-extension variant**. Natural JSON IR is
still a separate future reader.

## Connected physical PF

The existing physical-coordinate profile now has connected AC/DC qualification.
Three AC buses with an AC triangle connect through three VSCs to a DC triangle.
The corpus varies:

- Eight topologies: intact, AC-line outage, AC island, three AC islands, DC-line
  outage, isolated DC terminal, combined AC/DC separation, and converter outage.
- Fixed DC voltage plus fixed PCC P, all-droop DC control, or mixed fixed-voltage
  and droop control. A shared generator/converter AC-voltage group checks equal
  reactive injection increments with the opposite PCC-withdrawal sign.
- Both transfer-setting orientations, one/two DC poles, and four equivalent
  power/local AC/converter/DC voltage base configurations.

This gives 96 physical/control cases and 384 representations. Each has declared
and physically perturbed starts; neither is a reference-solved warm start.
AC island references are explicit study edits. The solver does not turn numerical
OPF anchors into PF dispatch controls. Non-reference generators retain their
specified Pg. Every DC island must retain fixed-voltage or droop authority.

Admission tests reject missing AC references and loss of the last DC voltage
controller, including an isolated terminal. Outage fingerprints reject starts
from the pre-outage topology. Three derivative points cover connected, separated
AC, and combined AC/DC island cases; voltage, DC power and reactive-sharing
corruptions fail the equations.

The independent phase-domain oracle checks station circuits, physical AC-line
currents and charging, DC Ohm's law, bus balances, total losses, controls and
reference/dispatch equations. It does not confuse a thermal-rating exceedance
with a PF electrical residual. Ratings remain post-solve reports.

## Fixed loss-mode PF

`FixedModePhysicalDcDroopSharedVoltageEqualIncrementV1` adds explicit
`AcDcStationLossMode` entries for every active station. `Rectifier` selects Crec
and requires valve AC withdrawal P >= 0; `Inverter` selects Cinv and requires
P <= 0. These are valve-side signs, independent of PCC/DC sign. Losses can make
those signs differ near idle. Zero valve P retains the declared curve, even
with nonzero reactive current; no curve is inferred from a solved sign.

Construction rejects missing, duplicate, inactive or unknown station selections
and an empty intersection between a mode's sign and declared valve-P bounds.
Existing profiles reject explicit mode fields, including empty arrays and null.
Mode entries are sorted by persistent component identity before persistence and
fingerprinting. All prior profiles omit the field, preserving their wire shape
and preparation fingerprints. An instance/result round trip retains modes;
MATPOWER export retains the two loss coefficients but does not encode solver
intent. The reference runner therefore consumes the separately recorded modes.

The native solver evaluates the smooth selected curve during Newton iteration,
then requires the final root to satisfy the selected mode's sign within numerical
tolerance. It does not search another mode, silently switch curves or claim no
root exists if the local solve fails. Mode-domain violations invalidate the PF
result; ordinary current/P/Q ratings remain reported after solving. Tests verify
that tightening current and valve-P ratings does not clip the point, while an
opposite mode refuses the root and invalidates a stored start fingerprint.

The corpus has 72 physical/control cases and 288 representations: three station
configurations (all bypassed, series elements, full station), two modes, normal,
zero and near-zero valve P, one/two poles, fixed-voltage/droop control, and four
base choices. An independently constructed circuit uses 0.3/1.3 ohm quadratic
coefficients. Four derivative points include the zero-power boundary on each
smooth selected branch; these are not derivatives through an automatic switch.

## Independent reference and evidence

Both reference runners build fresh PMACDC PF models from exported files. They
explicitly substitute physical transformer-bypass and DC-line rows and physical
MW/kV droop. Connected cases add the declared sharing allocation, remove duplicate
voltage equations only for solving, then restore them for validation. Fixed-mode
cases substitute the selected loss curve in MW/kA and check the valve-P sign;
they solve the same smooth extension as native Newton before restoring the mode
bound. Exact constant auxiliaries are presolved and restored. Source Imax repair
is undone. This is an explicitly modified physical/control reference, not a
claim of equivalence to unmodified source-pu PMACDC.

Both solvers' physical points are evaluated on every equivalent base. The Rust
phase-domain oracle independently checks native and reference solutions. Reports
retain source, harness and complete-report hashes. Local root agreement is not
a uniqueness, global convergence or operational-feasibility proof. No new
PMACDC equation bug was established.

| Gate | Connected PF | Fixed-mode PF |
| --- | ---: | ---: |
| Representations | 384 | 288 |
| Native solves | 768 | 576 |
| Reference solves | 768 | 576 |
| Native equivalent-base checks | 3,072 | 2,304 |
| Reference equivalent-base checks (both solvers) | 6,144 | 4,608 |
| Reference points checked on native bases | 3,072 | 2,304 |
| Reference corruptions rejected | 2,304 | 1,728 |

Maximum native residuals are 9.996e-11 (connected) and 6.702e-11 (fixed modes).
Maximum reference residuals are 1.267e-8 and 8.626e-9; physical coordinate
agreement across solvers/bases is within 2.928e-7 and 4.681e-8 respectively.
An additional 64 explicitly mixed rectifier/inverter connected/outage cases pass
independent phase-domain checks, including isolated terminals and inactive
stations. Modes are assigned by the study contract, not solved signs.

The full Tellegen suite passes 811 tests, with 7 existing ignores; the final six
focused tests also pass after adding mixed-station coverage. Numerical-feature
all-target Clippy, default all-target compilation, formatting, pin agreement and
EPL guard pass. Case5 retains eight fingerprints and passes 48 solves/checks and
96 corruptions; the existing physical PF corpus retains all 256 fingerprints and
passes its 1,024 solves/8,192 base checks. All 42 compatibility exports and records
are byte-identical.

Compact evidence: [connected](connected-qualification.json),
[fixed modes](fixed-qualification.json),
[PowerIO checks](milestones-powerio-validation.json), and
[native/regression checks](milestones-native-validation.json).
Full reports remain outside the checkout in `/tmp/tacdc-connected-pf-live` and
`/tmp/tacdc-fixed-pf-live`. Frozen PowerIO `0ed1174c` accepts the four existing
profiles and rejects the new instance/result profile. Only the unreleased schema
changes; 258 affected PowerIO tests pass. Documentation and C/Python lint pass;
three broad lint targets retain the previously recorded unrelated float-comparison
failures in PSS/E, PMD and synthetic-parity tests.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-pf-milestones-target \
  python3 evals/validation/acdc/pf/milestones.py /tmp/tacdc-connected-pf-live --suite connected
CARGO_TARGET_DIR=/tmp/tacdc-pf-milestones-target \
  python3 evals/validation/acdc/pf/milestones.py /tmp/tacdc-fixed-pf-live --suite fixed
```

## Next checkpoint: consolidate and review

The reader, reference-compatible VSC PF/OPF, physical-base electrical models,
connected physical PF/OPF and explicit fixed loss-mode PF/OPF now have independent
qualification. Keep the existing PRs draft while consolidating the accumulated
model changes into reviewable dependency/API/model/evidence units. Review the
canonical intent/result and profile contracts, the release capability table,
source-preserving refusal behavior, and the candidate PowerIO dependency before
release. Historical "next" statements in earlier evidence pages are superseded
by this checkpoint and the current status section in issue #819.

Separate later features remain: [proper saturation](SATURATION.md), automatic
mode reversal, valve-power droop/deadbands, LCC and explicit pole/return wiring,
generic rebasing/projections, larger-case sparse/performance work, natural JSON
input, and bindings/UI exposure of these new canonical solver profiles. None is
silently enabled by this milestone. Saturation needs a new solved control policy,
including redistribution and loss of island voltage support; it is not clipping
an already solved point.
