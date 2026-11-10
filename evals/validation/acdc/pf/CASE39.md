# Case39 PF with explicit controller and dispatch choices

This gate qualifies **a fixed-OPF-Pg dispatch plus an explicitly selected
DC-voltage controller**. It does not qualify unchanged case39 or a control-only
edit. The input is the PowerModelsACDC **MATPOWER-extension variant**; natural
JSON IR remains a future adapter.

Original `case39_10_he.m` has ten PCC-active-power converters and no DC-voltage
controller. The existing PF profile correctly refuses it. Separately recorded
trials select converter 1 or 8 while retaining the complete original dispatch.
Native and reference declared/flat/perturbed local trials fail to converge. This
is neither a proof of infeasibility nor evidence of a PMACDC equation bug.

For the positive study, generator Pg values are fixed inputs taken from the
already-qualified source OPF in `../case39/reference.json`, mapped by the checked
identity map. Only Pg is copied: no OPF voltage, angle, Qg, converter output,
current or solved PF point supplies a start. The reference-bus generator Pg is
free to balance losses under the existing PF policy. The qualification artifact
records all ten original/selected Pg values and hashes both provenance files.
This dispatch is a reproducible study choice, not a new automatic dispatch policy.

PowerIO remains pinned to `dbc52de6650e43efa734125e398994960d8c2a02` and PMACDC
0.10.1 to `9ada4082ac087aed649f48fb5f977680b0fe06c7`. The existing
`PowerModelsAcDc0101AcpVscEqualIncrementV1` profile and production solvers are
unchanged. The original source file is unchanged.

## Cases and acceptance

| Variant | Explicit change from the fixed-Pg study |
| --- | --- |
| Converter 1 controls Vdc | Select converter 1, retain Vdcset = 1.0079 |
| Converter 8 controls Vdc | Select converter 8 instead, retain Vdcset = 1.0079 |
| DC voltage | Converter 1 target = 1.02 |
| PCC active power | Converter 2 withdrawal increases by 10 MW |
| PCC reactive power | Converter 2 withdrawal decreases by 7 MVAr |
| Parallel DC-line outage | Disable one parallel 2–4 branch |
| Converter outage | Disable converter 10, retaining its DC bus |
| Tight ratings | Imax = 0.01 pu; generator P limits 0–1 MW, Q limits +/-1 MVAr |

All other selected controls and retained settings, including unused droop/Pdcset
declarations, remain unchanged. Each study uses declared, flat and deterministic
perturbed starts independently in each solver. All native points must satisfy
the parsed reference equations and all successful reference points the native
equations. Every variant requires at least two successful independent reference
starts; failed trials are retained and excluded from successful-solve counts.
Acceptance remains 1e-7 for cross-model constraints and electrical coordinates,
with phases compared modulo 2*pi; native convergence remains 1e-10. Ratings are
reported after solving and do not change the electrical point.

Direct Rust tests verify selected Vdc and PCC P/Q targets, nonreference Pg,
positive DC-line losses, summed converter/DC-line power balance, inactive
coordinates, source retention and stale-start refusal. Five preparation refusals
cover the original missing controller, duplicate controllers, controller outage,
an uncontrolled split DC island and a nonpositive Vdc target. Reference negative
controls corrupt the AC reference angle, PCC P, PCC Q and Vdc separately.

The Julia adapter uses pinned `build_acdcpf` with exact affine/auxiliary presolve
and a positive-voltage numerical domain. Original rows and bounds are restored
before checking acceptance. No rating, residual tolerance or electrical equation
is relaxed. The reference uses the existing adaptive-barrier settings. The
`dc_voltage` flat start stalls at a collapsed converter-voltage iterate despite
the independently obtained native point satisfying the reference equations.
Gradient scaling, smaller barrier/bound pushes and a limited-memory Hessian did
not resolve it; a monotone barrier also failed on the baseline declared start.
These exploratory logs are retained externally. None of those alternatives is
selected in the final harness. Local convergence from every start is not claimed.

## Results

Eight variants pass **24 native solves, 23 reference solves, 47 cross-model
checks and 92 corruption controls**. One reference trial (`dc_voltage/flat`)
returns `LOCALLY_INFEASIBLE`, with residual 3.21865. Its declared and perturbed
starts succeed; that status therefore does not establish infeasibility. All 24
native points satisfy reference equations. Maximum native residual is 8.510e-11,
reference residual 9.733e-11, reverse-check residual 1.616e-10 and electrical
coordinate difference 4.068e-9.

All six control-only native trials and six corresponding reference trials fail
locally. These retain original Pg and are separate from the positive fixed-Pg
study. Reference residuals range from 1.944 to 2.276.

The baseline converter-1 study reports **34 rating violations**, versus 59 with
artificially tight ratings; its electrical point is unchanged. These are PF
equation checks, not operational-feasibility or dispatch recommendations. Other
untightened variants report 30–34 violations. The full suite passes **795 tests,
7 ignored**; all-target Clippy, default all-target compile, formatting and pin/EPL
guards pass. The [case5 regression receipt](case39-case5-regression.json) records
48 successful solves/checks, 96 corruptions and eight unchanged fingerprints.
All 42 compatibility exports and their case records remain byte-identical.

Full generated cases, points, failed-trial iterates and logs stay outside the
checkout. [case39-qualification.json](case39-qualification.json) records compact
metrics, provenance, refusals and control-only trial outcomes.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-case39-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-case39-pf-live --suite case39
```

Case67 remains a separate next gate: define its AC-island PF reference/dispatch
and droop contract before numerical qualification. Physical-unit/fixed-mode PF,
saturation, automatic control switching, generic rebasing and physical pole/return
wiring are not established by this study.
