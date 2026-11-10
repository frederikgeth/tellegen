# Opt-in native DC-power-droop PF

`PowerModelsAcDc0101AcpVscDcDroopSharedVoltageEqualIncrementV1` adds type-3
DC-power droop to the existing shared-voltage PF allocation. PowerIO is pinned
to `70025e87d918be195362ca63a48c9a68d2dfb702`; the reference remains PMACDC 0.10.1
at `9ada4082ac087aed649f48fb5f977680b0fe06c7`. The input is the
**MATPOWER-extension variant**. Natural JSON IR remains a separate future adapter.

The existing two PF profiles retain their controls and refusals. Selecting the
new profile is required even when the network has no active droop converter;
the profile enters IR and preparation fingerprints, so starts cannot be reused
across profiles without an explicit new preparation. Existing case5 equations
and points are unchanged.

## Controls and admission

For every active type-3 station the native system replaces fixed PCC P with:

```
Pdc_pu - Pdcset_MW / Sbase_MVA + (Vdc_pu - Vdcset_pu) / droop = 0
```

Pdc is withdrawal from the DC grid into the converter. The slope is pu voltage
per pu power, and its derivative is dPdc/dVdc = -1/droop. It does not use the
opposite-sign source PCC generation or the valve active power. Targets and
unused settings remain retained in the network; no source field is overwritten
with a solved value. The analytic Jacobian and Hessian use the same native
expression machinery as the electrical equations.

An active droop controller requires a positive finite slope with a finite
reciprocal, a positive Vdc target and zero deadband. The equipment validator
already rejects nonfinite declarations. Type-4 valve-power droop remains
unsupported. Each connected DC island needs at least one active fixed-Vdc or
positive-droop controller and at most one fixed-Vdc controller. Controller
outages and DC connectivity are evaluated locally; remote droop cannot supply
an isolated fixed-P island. Inactive and unselected finite settings remain data.

AC islands still require exactly one declared reference with active generation.
The existing shared-voltage policy supplies equal reactive **injection**
increments across generators and voltage-controlling converters: delta Qg is
minus delta PCC withdrawal. Bus Vm supplies the common voltage target. Source-pu
station equations, Cinv loss selection, positive-voltage/current numerical
safeguards and reference Pg allocation are unchanged. No Newton solver tuning
or relaxation of convergence criteria accompanies the new profile.

## Case67 study and evidence

The eight variants reuse the [case67 reference contract](case67/README.md):
explicitly promote isolated bus 67 from PV to reference while preserving source
generator dispatch and converter controls; then vary droop Pdcset, Vdcset, slope,
droop-only DC authority, one DC-line outage, PCC withdrawal at bus 67 and ratings.
Original case67 still refuses for missing AC reference coverage. No OPF dispatch
or solved state seeds the native or reference PF runs.

Each variant uses declared, flat and deterministic perturbed native starts.
The independent reference receives the same *initial* point, not the solved
native point. All native points must satisfy the full parsed PMACDC equations,
and all successful PMACDC points must satisfy native PF equations. Reference
presolve, duplicate-voltage removal and numerical domain bounds are restored
before acceptance. Failed reference trials remain recorded separately; a variant
requires at least two successful independent reference starts.

Checks cover exact droop residuals and power/voltage corruptions, shared-Q
allocation, source retention, Pg67 following 800→810 MW PCC withdrawal, tight
ratings preserving the electrical point, 11 preparation refusals, and stale
starts after profile/control/topology edits. Six derivative points span the
baseline, changed slope and droop-only grid, each at its solution and a perturbed
point. They check full analytic gradients, Jacobians, Hessians and structural
zeros at both existing finite-difference steps under the unchanged 1e-6 threshold.

The [native qualification](droop-qualification.json) records the profile, source
and harness hashes, per-case outcomes, controls, original ratings and numerical
metrics. Full inputs, points, logs and failed iterates stay outside the checkout.

## Results

Eight case67 variants pass **24 native solves, 23 independent reference solves,
47 cross-model checks and 443 corruption controls**. All native solves converge
in five iterations, including the DC-line outage. Maximum native residual is
**3.664e-13**, native-point violation in the reference **4.547e-13**, and reference
residual/reverse-check violation **5.173e-12**. Maximum electrical coordinate
difference is **2.782e-10**.

The reference outage/declared trial returns `LOCALLY_INFEASIBLE` with residual
4.14549; its flat and perturbed starts succeed. Its initial point comes from the
native declared-start construction, which includes the declared current-loss
estimate, and differs from the earlier reference-only study's initializer.
The failed iterate remains in the hashed external reference report. The native
point for the same trial satisfies reference equations; this is a local solver
failure, not an infeasibility proof or a confirmed PMACDC equation bug.

The baseline reports **13 rating violations**, the outage 46, and tight ratings
49. Tight ratings leave the electrical point unchanged. The old case5 rerun
passes **48 solves/checks and 96 corruptions** with all eight fingerprints
unchanged; [the receipt](droop-case5-regression.json) records its hashes. All
42 compatibility exports and their case records remain byte-identical.

Full Tellegen suite: **801 passed, 7 ignored**. All-target Clippy, default
all-target compile, formatting and pin/EPL guards pass.

## IR and compatibility

PowerIO adds only the new enum token to the unreleased schema. Current instance
and result round trips pass. The frozen `dbc52de6` decoder accepts both existing
PF profile tokens and rejects the new token in instances and results. Relabeling
an active-droop instance as an old profile refuses during decoding. Published
schema snapshots remain unchanged.

PowerIO's **255 affected tests** and changed-library Clippy pass. All seven CI lint
targets were attempted: C ABI and Python targets pass; pre-existing PSS/E/PMD
float-comparison failures remain in workspace/no-SINCAL, and synthetic-facade
float comparisons remain in matrix-gridfm. Documentation build/tests and
all-feature rustdoc pass. [PowerIO evidence](droop-powerio-validation.json)
records test counts and log hashes.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-droop-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-droop-pf-live --suite case67
```

## Saturation follow-up

We probably want proper saturation later. The [saturation plan](SATURATION.md)
tracks limit-triggered control changes, redistribution, lost island voltage
support, active-set/complementarity choices and qualification. This profile
continues to report operating-limit violations after solving; it does not clip,
saturate, switch controls or assert operational feasibility. Generic rebasing,
physical/fixed-mode PF, valve-power droop and physical pole/return wiring remain
separate. No new PMACDC equation bug is established by this work.

## Physical-coordinate follow-up

The separately selected [physical PF profile](PHYSICAL.md) now qualifies local
voltage/impedance bases and physical droop scaling. This source-pu profile and
its historical evidence remain unchanged. Fixed loss-mode PF and saturation
remain separate follow-ups.
