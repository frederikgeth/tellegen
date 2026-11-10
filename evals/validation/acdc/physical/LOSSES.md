# Directional converter-loss contract and witnesses — 2026-11-10

This gate characterizes asymmetric converter losses and their reversal boundary.
It adds no production profile or solver switch. All three existing profiles,
PowerIO `ba1f9fc9`, and existing draft PR heads remain unchanged. The conclusion
is that directional losses need a separate, explicit policy; silently selecting
a coefficient inside the current smooth NLP would change both semantics and
regularity.

## Pinned implementation and signs

PowerModelsACDC 0.10.1 at `9ada4082ac087aed649f48fb5f977680b0fe06c7`:

- [constraint_template.jl, lines 74–80](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/core/constraint_template.jl#L74)
  unconditionally supplies `LossCinv` to the converter-loss constraint.
- [formconv/acp.jl, lines 7–12](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/formconv/acp.jl#L7)
  imposes `pconv_ac + pconv_dc = a + b*iconv + c*iconv²`.
- [data.jl, lines 764–765](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/src/core/data.jl#L764)
  explicitly warns that inverter losses are used when the two coefficients differ.

This is deliberate model behavior, not an established electrical-equation bug.
There **is a documentation mismatch**: [parser.md](https://github.com/Electa-Git/PowerModelsACDC.jl/blob/9ada4082ac087aed649f48fb5f977680b0fe06c7/docs/src/parser.md#L64)
labels `LossCinv` as “not yet implemented”, although the assembled ACP loss row
uses it. The checker inspects that actual row's coefficient, not just its source
name. No upstream report has been posted.

Our valve `pconv` and reference `pconv_ac` are positive from the AC station into
the valve. `pdc` is positive from the DC bus into the valve; their sum is positive
valve dissipation. PCC `pgrid` is positive from the AC grid into the station.
MATPOWER-extension `P_g` has the opposite PCC injection sign; the fresh-export
checker asserts this conversion. These are three distinct terminals, not
interchangeable direction indicators.

For a **proposed named valve-side policy**, positive valve AC withdrawal uses
the rectifier coefficient and negative withdrawal uses the inverter coefficient.
This is an explicitly chosen terminal convention. Small positive valve
withdrawal can coexist with positive DC withdrawal: both sides then supply
losses, so it does not imply net AC-to-DC export. A future policy name and
manifest must state that convention, rather than vaguely saying “power direction”.

## Physical loss contract

For line-to-line RMS valve voltage `Vll` in kV and three-phase `P,Q` in MW/MVAr:

```
I = sqrt(P² + Q²) / (sqrt(3)*Vll)                 [kA]
Lmode = A + B*I + Cmode*I²                       [MW]
Pvalve + Pdc = Lmode
Ibase = Sbase / (sqrt(3)*Vbase)
[a,b,c] = [A/Sbase, B*Ibase/Sbase, Cmode*Ibase²/Sbase]
```

There is no extra factor of three on the loss polynomial and no pole-count
multiplier on valve losses. Pole count applies to the DC network. The circuit
oracle reconstructs AC station losses separately, so `Ppcc-Pvalve` is not
mistaken for valve loss.

The witnesses demonstrate three distinct boundaries:

1. **PCC versus valve sign:** a full station with valve current
   `-0.00005-j0.08 kA` has negative valve P but positive PCC withdrawal. Selecting
   the valve curve from PCC `P_g` or `pgrid` gives the wrong named mode.
2. **Zero DC transfer:** `Pdc=0` requires `Pvalve=Lmode>0` and nonzero current.
   Both branch-specific idle roots are checked. Zero DC transfer is not zero
   valve AC power and not zero dissipation.
3. **Zero valve P with reactive current:** at `Vll=200 kV`, `I=0.08 kA`,
   `A=0.4 MW`, `B=0.7 kV`, `Crec=0.3 Ω`, `Cinv=1.3 Ω`, the two curves give
   **0.45792 MW and 0.46432 MW**. A hard valve-P sign switch has a **6.4 kW jump**.
   Near-zero ±1e-8 MW witnesses retain that jump. At zero current the quadratic
   difference disappears, but constant idle loss remains 0.4 MW, supplied here
   from the DC side. Out-of-service equipment is a separate status, not an idle
   operating point.

Zero-P witnesses deliberately evaluate **both** branch extensions without
choosing an automatic tie. Off-sign branch witnesses are explicitly labelled;
their electrical equalities do not establish directional operating feasibility.
Swapping the relative sizes of Crec/Cinv reverses the discrepancy; equal
coefficients eliminate it. The evidence therefore does not assume inverter
losses are always larger.

## MATACDC cross-check and caveat

The original [MATACDC manual](https://www.esat.kuleuven.be/electa/teaching/matacdc/MatACDCManual)
(Table 2, §4.1.2 and chapter 5) distinguishes rectifier/inverter coefficients and
defines converter AC power as injection into the AC network. A read-only audit
of the [official 1.0 archive](https://www.esat.kuleuven.be/electa/teaching/matacdc/MatACDC1_0)
found that `calclossac.m` selects the named rectifier coefficient for positive
`Pc`, the named inverter coefficient for negative `Pc`, and neither quadratic
term at exact zero. Its `runacdcpf.m` call site computes injection-oriented
valve power and `Pdc=Pc+loss`. Thus that released routine's naming/sign selection
conflicts with the physical mode names above, and its exact-zero rule removes
the quadratic loss despite nonzero reactive current.

This is a source-level finding in **MATACDC 1.0**, separate from PowerModelsACDC.
No MATLAB run or claim about other MATACDC versions is made. Do not treat that
routine as an unquestioned directional oracle. No source code from the archive
is copied into the repository; only provenance and independently derived
witnesses are retained. Archive SHA256:
`1cb50ce28cde3c2c0d9fba6063ee4c5b795b296315655ed0d98683f96257e01c`;
`calclossac.m` SHA256:
`d672c7f3ad0e9a654fba86b208b219672352ee148575081f918ba559deeb3f01`.

## Qualification and reproduction

Eight operating points × two selected curve extensions × three coefficient
pairs × full/bypassed stations × one/two poles × nine base representations give
**1,728 witnesses**. There are 648 witnesses matching the proposed named mode,
648 opposite-mode witnesses and 432 zero-mode boundary witnesses. This is
pointwise characterization, with **zero optimization solves** in this gate.

Native tests evaluate equivalent-base points, retain both coefficients through
IR, and check 32 derivative cases. All three existing profiles ignore changes
to Crec in their equations while retaining it in source/preparation identity.
The existing profiles still use Cinv; their behavior is not silently relabelled.

Julia builds fresh exported ACP models and restores declared Imax and SI
bypass/DC rows. It verifies that the unchanged source loss row has Cinv, matches
its signed residual to `(Cselected-Cinv)*I²`, then replaces **only** that row with
the explicit selected SI curve. Every other constraint and bound must hold.
Each altered-current negative control must fail that specific loss row, including
at zero initial current. Modified branch models are not called unmodified
PowerModelsACDC equivalence or directional OPF qualification.

| Result | Value |
| --- | ---: |
| Witnesses checked natively and independently | 1,728 |
| Expected inverter-only loss mismatches | 504 |
| Current-corruption controls | 1,728 |
| Maximum other reference residual | 7.276e-12 |
| Maximum selected-branch reference residual | 7.276e-12 |
| Maximum signed-loss residual disagreement | 2.146e-14 MW |

Acceptance is 1e-9 for other/selected-branch rows and 1e-10 MW for signed-loss
agreement. The [19,189-byte compact report](losses-qualification.json) records
all 48 operating/coefficient/branch groups, base configurations, source/report
hashes and scope. Full points and fresh exports remain in the external output.
All **39 affected AC/DC tests**, all-target Clippy, default all-target compilation
and fmt/pin/EPL guards pass. The generalized circuit helper reproduces all 42
original compatibility MATPOWER exports byte-for-byte. No production code or
PowerIO pin changes.

```sh
CARGO_TARGET_DIR=/tmp/tacdc-losses-target \
  python3 evals/validation/acdc/physical/losses.py /tmp/tacdc-losses-live
```

## Decision and next gate

A separate **explicit per-station mode policy** is needed before shipping
asymmetric directional losses. First qualify fixed rectifier and inverter
branches with signed valve-P domains, their declared coefficients, canonical
IR/profile tokens and distinct preparation fingerprints. This makes the
operating mode inspectable and keeps each branch's polynomial smooth. It does
not resolve automatic reversal: the zero-P overlap has different losses when
reactive current flows. Reversible optimization therefore needs an explicitly
specified disjunction or a separately justified continuous physical loss model,
with boundary acceptance tests; an undocumented sign-based `if` or smoothing
constant is insufficient.

That production policy and branch-constrained optimization are the next gate,
followed by native coupled PF controls. The present evidence establishes the
contract and the reason for separation, not a new usable directional solver.
