# Native coupled PF: case5 controls

For current support and remaining work, see [the completed physical PF milestones](MILESTONES.md).
This page retains the original case5 evidence; its historical next-step statements
are superseded by the current consolidation/review checkpoint.

Qualified 2026-11-10 against PowerModelsACDC 0.10.1 at
`9ada4082ac087aed649f48fb5f977680b0fe06c7`, Julia 1.12.6 and the pinned parent
Manifest. PowerIO is pinned to `e5a7c8b82845e32de7427f39ff8e63768f5f8bd2`.
The input adapter is the **MATPOWER-extension variant**, not the future natural
JSON representation of PowerModelsACDC's IR.

## Selected contract

`AcDcPfInstance` and `AcDcPfSolution` carry the required
`PowerModelsAcDc0101AcpVscEqualIncrementV1` profile. Its source-pu/Cinv electrical
equations reuse the existing compatibility implementation. PF has no objective.
Generator costs remain in the source owner but leave the private equation assembly.
Existing OPF profiles and default equations are unchanged.

- Exactly one declared AC reference with active generation per active AC island,
  and exactly one active converter DC-voltage controller per DC island.
- Active generators make their bus PV (except the reference); an empty declared
  PV bus becomes PQ under the existing reference bus-role convention.
- Generator and converter AC voltage controls use **bus Vm**, not generator Vg
  or retained converter Vtar. DC control uses converter Vdcset.
- Converter active/reactive power setpoints are **PCC withdrawals**. Supported
  combinations are P/Q, P/Vac, Vdc/Q and Vdc/Vac. Overlapping generator/converter
  or duplicate converter AC voltage authority is rejected.
- Nonreference generator Pg stays declared. Reference generators share changes
  from declared Pg equally; PV/reference generators share changes from declared
  Qg equally. This allocation is explicit because the original reference leaves
  some individual generator outputs undetermined.
- Ratings do not enter the PF solve. Original generator, voltage, branch and
  declared converter-current limits are checked afterward, including zero-width
  nonzero angle ratings. No clipping, saturation or PV/PQ switching occurs.
  MATPOWER's 0/0 angle-limit sentinel remains unrestricted.

`PreparedAcDcPf::new(instance)` produces an identity-keyed manifest and source/
profile fingerprint. `solve(AcDcPfStart::Declared, None)` returns the typed
solution, equation validation, iterations and separate limit violations. Flat,
explicit fingerprinted and typed instance-point starts are supported. A stale
fingerprint, incomplete/nonfinite point or nonphysical initial voltage/current
is rejected. Source declarations survive result IR round trips unchanged.

The small-system solver uses the analytic Jacobian, row-scaled dense pivoting
and damped Newton steps. It requires a square, nonsingular control system and
positive voltage magnitudes above 1e-8, with nonnegative current. Residual
acceptance is 1e-10. It checks cancellation and reports stalled, singular or
iteration-limited trials without claiming infeasibility. Even a converged warm
start must have a nonsingular Jacobian. Large-system performance is unqualified.

## Independent evidence

[qualification.json](qualification.json) contains compact results, source/report
hashes, controls, representative full native points and limit violations. The
runner retains raw inputs, full native/reference points and logs externally.

| Case5 variant | Qualification |
| --- | --- |
| Source | Original declarations and PV generator allocation |
| PCC active | Converter 1 withdrawal increased by 10 MW |
| PCC reactive | Converter 1 Q withdrawal decreased by 7 MVAr |
| DC voltage | Converter 2 Vdcset = 1.02 |
| PCC voltage | Converter 1 controls bus 2 at Vm = 1.01; Vtar = 1.08 retained |
| Dual voltage | Converter 1 controls both Vac and Vdc; converter 2 controls PCC P |
| Tight ratings | Imax = 0.01 pu, tight generator ratings and 5-degree fixed branch angle |
| Multiple reference generators | Second reference generator, declared Pg = 25 MW and Qg = 3 MVAr |

Each case is freshly exported, independently parsed and solved from declared,
flat and deterministically perturbed starts. The Julia model is built with
`PowerModelsACDC.build_acdcpf`; generator-sharing rows are independently formed
from parsed declarations, not native results. Every native point is checked in
this reference before the reference solve. Both directions validate all original
equations and reference bounds after restoring numerical presolve changes.

- **24 native + 24 independent reference solves**, **48 bidirectional checks**.
- **96 corruption controls**: wrong generator sharing with unchanged total Q,
  PCC active/reactive power changes, and a DC-voltage change.
- Maximum native residual: **3.329e-11**; reference and reference-in-native
  residual: **6.394e-11**. Maximum electrical coordinate difference: **5.744e-10**.
- Tight ratings produce the same electrical point as source within 1e-9 and
  report 15 violations. A converged PF is not evidence of an admissible dispatch;
  even the source case has two reported rating violations.
- Three derivative/identity-permutation cases pass. Typed IR round trips,
  zero-iteration warm starts, source retention, cost independence, stale starts,
  invalid starts, cancellation and zero-width angle ratings are covered in Rust.

Polar angles are compared modulo 2pi. Full raw reference points are retained;
integer phase windings and raw differences are separately reported. In this run,
one converter angle in the PCC-voltage declared-start reference differs by -4pi;
it passes the unchanged periodic equations. This is not raw-coordinate equality.

### Reference numerical preparation and failed trials

Unmodified Ipopt trials on the AC-voltage-control variants stalled, including
trials with negative voltage magnitudes, despite native points satisfying the
reference equations. Adaptive barrier settings and positive voltage domain
bounds alone did not resolve every start. Final qualification exactly eliminates
single-variable affine setpoints, fixed loads and unused VSC firing angles before
solving; all original rows and bounds are restored before acceptance. Temporary
1e-8 voltage lower bounds match the native numerical domain, not equipment
ratings. The saved solver options are in the report. No reference starts use
native solved points. Failed trials are not counted as passes or infeasibility
proofs, and no new upstream electrical-equation bug is established.

## Compatibility and remaining work

Full Tellegen suite: **787 passed, 7 ignored**. All-target Clippy, default all-target
compile, formatting and pin/EPL guards pass. The old case5 fingerprint and all
42 compatibility export/native records remain unchanged. PowerIO passes 219
affected tests and changed-library Clippy; all seven CI Clippy targets were
attempted. Existing PSS/E/PMD/synthetic-parity float-comparison failures remain;
all C ABI variants and Python pass.

Only the unreleased PowerIO schema changes. A full frozen `27aae3b3` decoder
accepts existing OPF IR and rejects both new PF structural variants. The current
decoder checks required capability/profile, controls and complete point shape.

Next: qualify case24 and larger PF systems, then connected control/outage and
island cases. Physical-unit/fixed-mode PF, generic rebasing, droop/deadbands,
automatic loss reversal, reactive saturation, LCC, physical pole/return wiring,
bindings and sparse/performance work remain separate. Current evidence is a
case5 local-root qualification, not a proof of uniqueness or global convergence.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-native-pf-target \
  python3 evals/validation/acdc/pf/run.py /tmp/tacdc-native-pf-live
CARGO_TARGET_DIR=/tmp/tacdc-native-pf-target CARGO_INCREMENTAL=0 \
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test -p tellegen --features 'acopf mc-opf conic schema' --locked --offline
```


## Follow-up: case24 and static islands

The [case24 gate](CASE24.md) now qualifies explicitly selected control/outage
variants, records the original source's voltage-control overlap, and adds a
reported near-idle current safeguard to the native numerical solver. The
[case5 rerun](case5-recheck.json) retains all eight PF fingerprints and passes the
same 48 solves, 48 cross-checks and 96 corruption controls. The historical report
above describes the initial numerical implementation; control/equation profiles
remain unchanged.


## Opt-in shared voltage authority

[Shared-voltage allocation](SHARED-VOLTAGE.md) now qualifies original case24 without
editing source controls, using a new explicit profile. This page's original
profile still rejects overlap; its fingerprints and behavior are unchanged.

## Opt-in physical-coordinate PF

[Physical-coordinate PF](PHYSICAL.md) now adds explicit local voltage/impedance
bases and physical droop scaling. Its 256 representations pass 512 native and
512 independent reference solves, with 8,192 equivalent-base checks. Existing
source profiles remain unchanged. Saturation and fixed loss-mode PF remain
separate follow-ups.
