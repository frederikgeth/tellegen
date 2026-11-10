# AC/DC implementation review map

The numerical milestones are implemented and qualified on the existing development
branches. This checkpoint consolidates current API documentation and fixes
integration gates; it introduces no numerical model, profile or solver change.
Start with the [canonical API](CONTRACT.md), then review the units below with their
tests and evidence. Earlier evidence pages retain their historical pins and next
steps; this page and issue [PowerIO #819](https://github.com/eigenergy/powerio/issues/819)
describe the current sequence.

## Dependency and release boundaries

| Layer | Review baseline and current state |
| --- | --- |
| Diagram | Tellegen #300 remains draft at `78976ef1f9a06dab3ffd9afe3adb86cc8dbac6e8`. |
| Equipment/ACDC | Tellegen #358 remains draft at `e814b78549fb5fe48112bc5a72f6732dd115756f`, the native model branch's base. |
| Reader | PowerIO #822 remains draft at `49e8eacb45b2051de91f917815a26706b8a30a07`; its authorized single-PR scope is the MATPOWER-extension reader/equipment/IR. |
| Canonical model dependency | `codex/819-acdc-models`, runtime revision `3e41473984585c0b38560f47ba3e0c2d930b029b`; documentation/test/gate consolidation through `832ae2b312e4a871ca5088ea26d58746d5a7d213`. |
| Native consumer | `codex/acdc-native-case5`, numerical checkpoint `2b97b10594bda49d0ca9734824bbae77d6a70141`, plus this documentation follow-up. |

Tellegen retains its exact PowerIO runtime pin `3e414739`; this consolidation's
PowerIO Rust changes are rustdoc, a documentation test and test-only lint cleanup. Its only schema change
is that description. No dependency repin or fingerprint change is needed.
The input format remains the MATPOWER-extension variant; the natural PMACDC JSON
serialization adapter remains separate.

These are proposed review units, not newly opened PRs or independently built split
branches. Preserve coherent APIs, tests and schema snapshots when constructing
those branches. PowerIO contributions target `main` sequentially after dependencies
land; review size and grouping belong in #819 before requesting model review.
The reader's single-PR agreement does not make the entire model stack one PR.
Respect the repository's default limit of three PRs awaiting review.

## PowerIO review units

Commit ranges below use Git's exclusive-left, inclusive-right notation. Inspect
the endpoint diff for each unit; carry the consolidation's relevant docs/gates
into the first unit introducing each type. All units depend on the reader.

| Unit | Historical range | Main review questions |
| --- | --- | --- |
| P1: canonical OPF intent/preparation/result and numerical anchors | `49e8eacb..552fc413` | Immutable source ownership, complete identity/units, all equipment blocks, IR capability/refusals, anchors without source-role edits. |
| P2: explicit physical/current/loss policies | `552fc413..27aae3b3` | Source-pu compatibility unchanged; declared current vs local bases; from-side DC resistance; complete fixed modes and valve-P bounds. |
| P3: PF intent and control allocation | `27aae3b3..70025e87` | PF separate from OPF; generator increments, shared-Q signs, AC/DC island authority, droop units/refusals, stored result semantics. |
| P4: physical and fixed-mode PF | `70025e87..3e414739` | Profile-derived coordinates, complete modes, strict field presence including null, unchanged older encodings. |

Primary source paths in PowerIO:

- `powerio-prob/src/acdc.rs` and `acdc_pf.rs`: canonical instances, profiles,
  points, results, validation and ownership.
- `powerio-matrix/src/acdc.rs`: complete OPF preparation and effective policies.
- `powerio/src/stored/{dto,convert}.rs` and `value.rs`: structural IR dispatch,
  strict field/profile behavior and capability guards.
- `powerio/tests/acdc_model.rs`: round trips, preserved declarations, refusal
  paths and profile selection; `docs/schema/pio-ir/2/unreleased/schema.json`,
  `docs/src/ir-reference.md` and structural/documentation gates must agree.

Public Rust types and IR transport do not imply new typed C/Python/Julia APIs.
In particular, IR tags such as `"powerio.AcDcPfInstance"` are not Python exports.
No numerical PF solver or standalone public PF matrix builder is added to PowerIO.

## Tellegen review units

| Unit | Historical range | Main review questions and evidence |
| --- | --- | --- |
| T1: native equations, derivatives and canonical OPF | `e814b785..1a0a566f` | Complete station/AC/DC equations, typed coordinate adapter, local solver acceptance, numerical anchor policy; case5/24/39/67 and derivative/negative controls. Depends on P1. |
| T2: physical/current/fixed-mode OPF | `1a0a566f..c48e6b77` | Units and unequal bases, bypass, mesh and AC/DC outages, Crec/Cinv and zero-power boundaries. Depends on P2. [Coordinates](physical/COORDINATES.md), [connected](physical/CONNECTED.md), [fixed modes](physical/FIXED-MODES.md). |
| T3: native PF and explicit controls | `c48e6b77..0a81eca3` | Newton/current retraction, generator and shared-Q allocation, source/study distinctions, droop and island authority. Depends on P3. [Case24](pf/README.md), [shared voltage](pf/SHARED-VOLTAGE.md), [case39](pf/CASE39.md), [case67 droop](pf/DROOP.md). |
| T4: physical and fixed-mode PF | `0a81eca3..2b97b105` | Physical droop/base conversion, connected/outage controls, mode-domain acceptance vs rating reports, strict stored modes. Depends on P4. [Physical](pf/PHYSICAL.md), [connected/fixed milestones](pf/MILESTONES.md). |

Primary source paths under `crates/tellegen/src/model/acopf/`:

- `equivalent.rs`: full-station equations, policy/control rows and native checks.
- `equivalent/coordinates.rs`: complete typed-to-native point conversion.
- `equivalent/derivatives.rs`: finite-difference/structural derivative checks.
- `equivalent/opf.rs`: manifests, starts, fingerprints and local OPF acceptance.
- `equivalent/pf.rs` and `equivalent/pf/newton.rs`: controls, fingerprints,
  square PF solve/current retraction, final mode domain and rating reporting.
- The small changes in `acopf.rs`, `hybrid.rs`, `model/mod.rs` and the public
  `ac_dc.rs` facade also need review for effects on existing solver paths.

The [file inventory](review-files.json) records exact snapshot diffs, separating
runtime source, tests, evidence and integration files. Evidence volume is not a
substitute for inspecting equations and negative tests. Historical commit ranges
include interim dependency pins; extracting/reordering units requires validating
each resulting build against its matching PowerIO APIs. Do not assume each
proposed unit can be blindly cherry-picked onto current main.

## Consolidation findings and checks

Five integration failures were reproduced during this pass and corrected:

1. The IR-reference test lacked PF instance/result tables and OPF `loss_modes`.
2. The structural-type parity gate omitted both PF types.
3. Bare dotted IR tags were mistaken for Python attributes by the symbol gate.
   Quote wire tags explicitly; the IR-reference test now accepts that exact form
   while still checking all types and fields. The Python-symbol gate is unchanged.
4. Public PF prose/rustdoc used vocabulary rejected by the terminology gate.
   Corrected the wording and regenerated the unreleased schema description.
5. A targeted matrix-enabled lint run exposed a missing semicolon in a PF
   refusal test, previously masked by broader unrelated lint failures. Added it
   without changing test behavior.

The reader guide and canonical API page also incorrectly described implemented
PF/physical/droop features as future work. They now distinguish four OPF and five
PF profiles, compatibility from physical policies, PF operating ratings from
mode-domain constraints, and explicitly modified reference studies from unmodified
PMACDC. No compatibility equations, encodings or runtime policies changed here.

This pass runs IR/model tests with matrix preparation enabled, schema regeneration,
structural-type/symbol/terminology/release-version gates, mdbook build/test and
all-feature workspace rustdoc with warnings denied. All seven Clippy configurations
are attempted. The three broad configurations retain known unrelated strict
float-comparison errors in `powerio-dist/tests/pmd_math.rs`,
`powerio-tx/src/format/psse.rs` and `powerio-matrix/tests/synth_facade_parity.rs`.
The focused matrix-enabled IR/model lint, C ABI configurations and Python extension
pass. Diagnostic parity passes for
core/stored/C/Python; Julia wrapper parity is skipped because `POWERIO_JL` is unset.
See [this pass's results](review-validation.json) for exact commands and statuses.

Numerical evidence is carried forward, not rerun for prose/test-gate changes:
the latest milestone records 811 passing Tellegen tests (7 existing ignores),
connected PF's 768 native + 768 reference solves and 12,288 base checks, fixed-mode
PF's 576 + 576 solves and 9,216 base checks, plus corruptions and mixed-mode roots.
Eight case5 and 256 physical-PF fingerprints and 42 compatibility exports/records
were unchanged at that checkpoint. Full reports remain outside the checkout;
compact evidence and provenance hashes are committed.

## Remaining gates and next work

1. Agree the review grouping in #819, then construct the first coherent model
   review unit on the reviewed reader/equipment foundation. Leave existing PRs
   draft until intentionally ready; this pass moves none of their heads.
2. Run the required integration gates on the eventual split branches. Resolve
   the known unrelated PowerIO lint failures before calling the whole CI green.
   This pass does not claim Tellegen's complete `just ci`, frontend/WASM/Python
   packaging or release qualification.
3. Publish PowerIO with the accepted new APIs before Tellegen release packaging.
   `gates-rust.yml` runs `cargo package` with all shipping numerical features;
   packaged dependencies resolve from crates.io without the development patch.
   The current candidate pin is development evidence, not a release-ready dependency.
4. After review, choose a separate milestone for [proper saturation](pf/SATURATION.md)
   or broader models. Saturation needs redistribution and loss-of-voltage-support
   semantics, not clipping. Automatic mode reversal, type-4 droop/deadbands, LCC,
   explicit pole/return wiring, generic rebasing, larger-case sparse/performance
   work, natural JSON input and bindings/UI remain separately scoped.

No additional PMACDC issue is established by this consolidation. Existing reports:
[documentation #146](https://github.com/Electa-Git/PowerModelsACDC.jl/issues/146),
[Imax #147](https://github.com/Electa-Git/PowerModelsACDC.jl/issues/147), and
[directional loss #3](https://github.com/Electa-Git/PowerModelsACDC.jl/issues/3#issuecomment-6765392053).
