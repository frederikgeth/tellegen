# Physical local-coordinate profile — 2026-11-10

`PhysicalCoordinatesAcpVscV1` explicitly interprets local voltage and impedance
bases. It uses the declared-current policy and retains the inverter loss model
and reference auxiliary bounds. The compatibility and declared-current profiles
keep their source-coordinate equations. PowerIO's canonical contract and
preparation are pinned at `ba1f9fc90de768e53aaace1c21e68b0d64759713`.

## Coordinate contract

| Quantity | Physical interpretation |
| --- | --- |
| AC PCC voltage | pu on its AC bus line-to-line kV base |
| Filter and valve voltage | pu on station `ac_base_kv`, line-to-line kV |
| Transformer/reactor R/X | Converter-side ohms divided by `V_converter_base²/S` |
| Enabled transformer tap | Physical turns ratio times `V_converter_base/V_PCC_base` |
| Transformer bypass | Ideal unity-ratio physical connection; `V_PCC_base*vm_PCC = V_converter_base*vm_filter`; retained tap/R/X ignored |
| Reactor bypass | Common filter/valve base, equal physical voltages; retained R/X ignored |
| Filter susceptance | Converter-base pu, ignored when disabled |
| Valve current | AC RMS pu on `S/(sqrt(3)*V_converter_base)` kA; exact declared bound |
| DC voltage | pu on each node's per-pole kV base |
| DC resistance | Per-pole ohms divided by **from-node** `V_from_base²/S` |
| Power | Common owner MVA base; terminal injections and valve signs unchanged |

For `k=V_to_base/V_from_base`, DC terminal injections become
`pf=n/r * vf*(vf-k*vt)` and `pt=n/r * (k*vt)*(k*vt-vf)`.
Reversing branch orientation requires converting resistance to the new from
base as well as exchanging terminal quantities. Changing only base metadata
describes different physical hardware under this profile.

Preparation/manifests record the derived coordinate policy, all station port
and impedance bases, physical turns ratio when enabled, and the branch
resistance reference side/voltage ratio. Active AC bases must be positive and
finite; nonrepresentable ratios refuse. The required persisted profile token
makes old decoders reject unsupported semantics. Existing v3 fingerprints
include this profile, so old profile identities remain unchanged while new
profile starts cannot be reused under a different profile.

This is an explicit interpretation of supplied data. It does not infer the
impedance/tap convention of an arbitrary imported source, change owner fields,
provide a generic rebasing API, qualify independent filter/valve bases or
construct physical pole/return topology. Native PF, directional losses and
optimizer invariance remain separate gates.

## Independent checks

The phase-domain construction from [BYPASS.md](BYPASS.md) supplies the same
384 physical points. The new profile must accept all of them, including the
48 unequal-PCC bypass and 48 unequal-DC witnesses. Both source profiles are
also evaluated at every point and must retain their original expected outcomes.

Julia parses fresh MATPOWER-extension exports into the pinned PowerModelsACDC
model. The checker restores declared Imax before construction, verifies the
original source-coordinate residuals, then replaces only bypass magnitude and
DC Ohm-law equations. The replacement equations use physical kV, MW and ohms
from parsed declarations, independently of native per-unit ratio coefficients:

```
V_PCC_base*vm_PCC = V_converter_base*vm_filter
S*pf = n*(V_from_base*vf)*(V_from_base*vf - V_to_base*vt)/R_ohm
S*pt = n*(V_to_base*vt)*(V_to_base*vt - V_from_base*vf)/R_ohm
R_ohm = r_pu*V_from_base²/S
```

Every variable, constraint, bound and objective is evaluated. Each reference
replacement is recorded; these are **modified-reference** results, not
unmodified PowerModelsACDC equivalence. A sqrt(3) current corruption must violate
the previously satisfied current equation in every case. The 108 within,
binding and violating declared-current cases also run under the new profile.

Native regressions cover all 384 typed-instance/point IR round trips, distinct
profile identities, 48 branch reversals with correct/wrong impedance bases,
16 dense Jacobian/Hessian checks and deliberate base-metadata corruption.

## Results

All **384** points satisfy the new native equations/bounds within `4.963e-14`.
Modified Julia's maximum residual is `7.276e-12`; its replacement rows use SI
MW/kV units, while native rows use per-unit residuals. Objective differences
are at most `1.137e-13`. All 96 original unequal-base failures become feasible;
both old profiles and the original Julia equations retain their expected
failures. All 384 current-equation corruption controls are detected (minimum
violation `0.264576`). The [14-group bypass record](coordinates-bypass.json)
is 31,074 bytes and includes full-report/input hashes.

The [108-case current record](coordinates-current.json) has 36 within-limit,
36 binding and 36 deliberately violating cases. Only the last group fails the
current bound; every other row/bound passes within `7.276e-12`. This confirms
that weighted coordinate equations do not bypass declared physical limits.

All previously recorded numerical fields in the 42 compatibility and 108
declared-current cases reproduce exactly, as do all 14 original bypass groups.
A fresh case5 pipeline preserves compatibility fingerprint
`sha256:7c58f42f6628cd17c86fee45f95dea7b0366ec555b29eb3f39b04542660b3912`.
All four native corpus regressions pass in the full suite: **769 tests passed,
7 ignored** with `acopf mc-opf conic schema`; all-target Clippy, formatting,
pin and EPL guards pass. PowerIO has 496 affected tests passing and changed-code
Clippy clean; its three broad Clippy configurations retain previously documented
baseline float-comparison warnings. The C ABI configurations and Python pass.

These initially qualify analytical primal points, coordinate conversion and
derivatives. The follow-up [optimized-base gate](OPTIMIZED.md) separately
qualifies local optimized solutions for explicitly converted synthetic networks;
new PF controls remain unqualified.
Existing PRs remain draft, and no upstream bug report was filed.

## Reproduce

```sh
CARGO_TARGET_DIR=/tmp/tacdc-coordinates-target python3 evals/validation/acdc/physical/run.py /tmp/tacdc-coordinates-bypass --suite bypass --profile physical-coordinates
CARGO_TARGET_DIR=/tmp/tacdc-coordinates-target python3 evals/validation/acdc/physical/run.py /tmp/tacdc-coordinates-current --profile physical-coordinates
```

Generated cases and complete reports remain outside the checkout. Compact
qualification records retain every coverage group, input/report hashes and
representative results. Remove the temporary target after validation; retain
reports and logs.
