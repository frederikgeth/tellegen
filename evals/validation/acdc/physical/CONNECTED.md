# Connected AC/DC networks and outages — 2026-11-10

This gate extends the [DC mesh qualification](MESH.md) with an AC triangle,
equipment outages and independently referenced AC islands. It changes no
production equation, compatibility profile, schema or PowerIO pin (`ba1f9fc9`).
Work remains on `codex/acdc-native-case5`; the existing PR heads remain draft.

## Physical fixture and coordinate contract

AC buses 11/12/13 each retain their generator, load and VSC; DC nodes 21/22/23
form the previous 10/15/20-ohm triangle. Three AC lines connect 11→12→13→11.
Each has series impedance `1.2+j12` ohms, total charging `j0.00002` siemens,
a 45 MVA rating at both ends and ±30° angle limits. Station masks `[7,3,0]`
cover a full station, a disabled filter and a completely bypassed station.
Source bus roles are REF/PV/PV throughout; bus 11 is the only declared anchor.

The AC pi circuit has unity **physical** turns. Its per-unit impedance uses
the receiving bus base, `Zbase=Bto²/S`, charging is `Bsi*Zbase`, and the normalized
tap is `Bto/Bfrom`. Consequently changing PCC coordinate bases does not introduce
a physical transformer. Converter and DC quantities use the previously documented
[explicit port/impedance bases](COORDINATES.md). This fixture builder is not a
generic imported-network rebasing API.

| Topology | AC equipment | DC equipment | Numerical AC anchors |
| --- | --- | --- | --- |
| 0 | Intact triangle | Intact triangle, all VSCs | 11 |
| 1 | Line 11–12 out; connected path | Intact | 11 |
| 2 | Only line 11–12 active; bus 13 island | Intact | 11, 13 |
| 3 | All AC lines out | Intact | 11, 12, 13 |
| 4 | Intact | Line 21–22 out; connected path | 11 |
| 5 | Intact | Only line 21–22 active; node 23 isolated | 11 |
| 6 | Only line 11–12 active | Only line 21–22 active | 11, 13 |
| 7 | Only line 11–12 active | DC triangle intact; VSC 33 out | 11, 13 |

Each topology combines cheap generation at bus 11 or 13 and one/two symmetric
poles: **32 physical networks**, each on the four MVA/voltage-base representations
in MESH.md. Outages change only equipment statuses. Inactive AC lines, DC lines
and converters remain in source/IR data but have no optimization coordinates.
DC node voltages remain, including the unconstrained operating voltage of an
isolated zero-withdrawal node. That voltage need not match between solvers.

The two starts are a physical zero-transfer/local-generation point and a
deterministic perturbation. For an isolated DC node with an active all-bypassed
VSC, zero DC transfer still requires positive converter idle losses. Both starts
initialize its current from the small physical root of
`1.3*I² + (0.7-sqrt(3)*Vll)*I + 0.4 = 0`, with consistent active powers and local
generation. This uses declared coefficients, not a solved oracle. Both initial
points and every physical variable bound are checked across bases.

## Independent checks

Julia parses fresh MATPOWER-extension exports, independently checks every outage,
endpoint and source bus role, and derives AC components using AC branches only.
It adds zero-angle constraints only on components lacking a declared reference;
the resulting anchor provenance must exactly match Tellegen's manifest. No source
PV bus is promoted and no PF dispatch control is inferred.

As in MESH.md, the reference explicitly restores declared Imax, substitutes SI
bypass/DC equations, normalizes those rows for optimization and presolves proven
constant/unused auxiliaries. Every accepted point is checked after restoring the
original auxiliary rows/bounds, with substituted residuals measured in original
units. The reference additionally retains its redundant AC P/Q component bounds
and checks they equal ±45 MW/MVAr, implied by the common apparent-power circles.
This is a modified physical-coordinate reference, not unmodified PowerModelsACDC
equivalence. All variables are accounted for, including nine fixed-load and two
or three unused firing-angle auxiliaries.

Direct phase-domain reconstruction independently checks both AC pi terminal
powers, charging, thermal circles and I²R losses; station voltages, complex
currents and losses; DC Ohm laws and nodal balance; AC nodal and total network
power balance. Both solvers' points must satisfy every equivalent-base model and
have the same cost. Corruption controls change a DC voltage, PCC power or AC
angle. Primal equality, global optimality and unique dispatch are not assumed.

Native regressions also cover source/IR preservation, stale-start rejection,
equipment row permutations and eight derivative cases. Rotating bus 13 and its
station together leaves all electrical rows unchanged when it is an AC island,
even though the DC mesh is connected; only its numerical anchor fails. The same
rotation in the connected AC triangle breaks branch equations. A separate
negative case disables the sole generator of the isolated, converter-less
65 MW load: its active balance is a nonzero constant and solving is rejected.
No automatic shedding or hidden slack supply is introduced.

## Numerical qualification

The initial generic native zero-transfer start failed at topology 5 / cheap bus
13 / two poles / voltage-only base change. The declared idle-loss initialization
above resolves the native matrix without changing production solver settings.
The initial Julia monotone-barrier run still failed restoration at the same
physical scenario on the reference base; an adaptive-barrier trial failed locally
on the voltage-only representation, despite the independently feasible native
point. These trials are not counted as successful solves or infeasibility proofs.

The final reference uses uniform `bound_push=1e-8` and `bound_frac=1e-8` to keep
Ipopt's initial interior displacement small near the idle current's lower bound,
retaining monotone barrier, no automatic NLP scaling and all preceding tolerances.
The default 0.01 push displaced that declared initialization. No bound or
acceptance tolerance is relaxed. This is an initialization/conditioning finding,
not a confirmed upstream electrical-equation bug. Diagnostic logs remain outside
the checkout; no upstream issue was filed.

## Results and reproduction

All **256 native + 256 independent Julia local solves** pass over 128
representations. There are **3,072 cross-formulation/base checks**, another
1,024 native-to-native base checks and 768 Julia corruption controls. All four
base representations preserve each physical problem's cost and feasibility.

| Metric | Maximum |
| --- | ---: |
| Native own-model violation | 3.305e-10 |
| Native KKT error | 8.915e-9 |
| Julia own-model violation, original units | 3.411e-11 |
| Cross-base violation in Julia, original units | 1.322e-9 |
| Julia points in native models | 1.162e-10 |
| Native–Julia objective difference | 1.920e-8 |
| Cost range across starts/solvers/bases | 2.471e-8 |
| Native direct physical residual | 6.012e-10 |
| Julia direct physical residual | 3.227e-9 |

Acceptance remains `1e-7` for constraints, `1e-6 + 1e-8*|cost|` for cost and
`1e-6` for the direct kV/kA/MW/MVAr residuals. The [83,183-byte compact report](connected-qualification.json)
records anchor provenance, coordinate counts, source/report hashes, physical
dispatch and differences. Cost changes relative to intact topology range from
0 to 387.115 here; monotonicity is not an acceptance assumption. The maximum
DC-node-23 voltage spread is 1.264 kV; differing feasible primal values are
retained, not averaged or treated as failures.

All **36 affected AC/DC tests** pass with `acopf mc-opf conic schema`, including
four new connected/outage regressions and the four-case native corpus. All-target
Clippy, default-feature all-target compilation, formatting and pin/EPL guards
pass. This follow-up reruns affected suites; the historical 769-test full-suite
result is not a new full-suite claim. The extracted shared numerical helper also
reproduces the previous mesh's full Julia report byte-for-byte (192 solves and
1,536 cross-checks).

```sh
CARGO_TARGET_DIR=/tmp/tacdc-connected-target \
  python3 evals/validation/acdc/physical/connected.py /tmp/tacdc-connected-live
```

The runner invalidates old reports, generates all native solves/exports, runs
Julia and verifies its points natively before aggregating the compact report.
Full exported cases, points and logs remain outside the checkout. Remove the
owned Cargo target after checks, retaining that evidence.

This qualifies local OPF on these connected AC/DC networks and explicit static
outages. It does not implement an N−1 security-constrained dispatch, dynamic
islanding/resynchronization, automatic load shedding, global infeasibility
certification, physical pole/return wiring or native coupled PF. Generic rebasing,
directional losses and PF controls/droop/deadbands remain separate gates. A useful
next gate is an explicit directional-loss contract and rectifier/inverter
reversal witnesses before extending physical models further.
