# Experimental coupled OPF contract

The coupled intent, typed operating point/result, IR persistence and numerical
preparation now live on PowerIO branch `codex/819-acdc-models`, based on the
unchanged reader commit `49e8eacb`. Tellegen owns the expression DAG, derivative
checks, solver starts/recovery and validation diagnostics. Existing PRs stay draft.

## Input and selection

`PreparedAcDcOpf::new(AcDcOpfInstance)` accepts canonical PowerIO intent.
`AcDcOpfInstance::from_network(network, profile)` requires the explicit
`PowerModelsAcDc0101AcpVscV1` profile. Stored intent rejects unknown profiles/fields. This closed selection fixes:

- the polar AC/full VSC station equations and source pu coordinate convention;
- inverter losses in both directions, reference Imax repair and auxiliary bounds;
- generator-cost minimization with free generator/converter dispatch, without
  PF setpoint constraints;
- canonical AC reference-angle handling, with DC voltages constrained by limits
  rather than PF reference controls;
- rejection of unsupported/mixed representations and normalized/base-mismatched
  input, using the existing equation-model checks.

A preparation owns immutable native expressions. Its manifest records the
profile/revisions, base MVA/pole count, source identity maps, named variables and
constraints with effective bounds, and declared/effective station parameters.
Absent bounds serialize as null; solver infinity sentinels are not a public
limit declaration. Source equipment is never overwritten with effective values.

The model fingerprint hashes a versioned formulation tag, the complete serialized
source network **including DC/station declarations**, and the selected profile.
Source edits invalidate a supplied operating-point start even if native variable
names happen to remain unchanged. This deliberately conservative content identity
is not a proof that two differently serialized source cases are electrically
inequivalent. A formulation change requires a new version/fingerprint tag.

## Starts and results

`AcDcOpfStart` selects:

- `Declared`: AC preparation defaults and source DC voltage guesses; approximate
  station port powers from the retained PCC settings, AC current from apparent
  power, and DC power from the loss balance. These are numerical guesses only.
- `Flat`: unit voltage magnitudes, zero angles/port powers/reactive generation,
  midpoint generator P bounds and 0.1 pu valve current. The nonzero current avoids
  a zero current-equation Jacobian at the cold start.
- `InstancePoint`: the complete typed point supplied on the canonical PowerIO
  instance, converted from MW/MVAr to private solver pu coordinates.
- `Point`: a diagnostic exact, finite, complete identity-named point with a matching model
  fingerprint. Missing/extra/nonfinite values or stale fingerprints refuse.

Constructed starts are projected to variable bounds; supplied points are retained
exactly. `initial_point` records the point given to POUNCE; the optimizer may
apply its standard interior-bound initialization. Starts do not change dispatch
constraints and cannot authorize an AC-only projection.

`solve(start, cancel)` uses the existing POUNCE restoration/recovery driver and
cancellation wrapper. The result records requested starts, the complete native
point, selected/base status, recovery attempts, total iterations, restoration
counts, solver/options, constraint/KKT diagnostics and native validation rows.
It also returns `solution: powerio::AcDcOpfSolution`, with the shared canonical
instance and typed `AcDcQuantity`/identity values in MW/MVAr, pu and radians.
PowerIO IR persists these physical values; private solver-name maps remain
Tellegen diagnostics. The example writes `native-{start}.pio.json` alongside
its report. Integration tests reload these and re-evaluate them as supplied starts.
The result is returned only after a successful local solver status, finite point
and objective, native violation <=1e-7, solver constraint violation <=1e-7, KKT
error <=1e-6, and agreement between solver and re-evaluated native objective.
Local failures are errors, never proofs of infeasibility. Global optimality and
qualified dual/sensitivity results are not claimed.

The API is available only with the native `acopf` feature. Existing browser,
CLI, server and Python adapters do not acquire a new solve mode. Ordinary AC-only
PowerIO constructors continue to reject the equivalent equipment owner.

## Independent acceptance

The external `cross-check.jl` instantiates the original PowerModelsACDC ACP OPF
without solving it, maps every native coordinate, and evaluates constraints,
variable bounds and objective at each native solution. Case5 maps 95 native
coordinates into 107 reference variables; case24 maps 657 into 766. All reference
variables must be accounted for: fixed-demand/zero-curtailment auxiliaries and
unused VSC firing angles are reconstructed from source declarations and recorded.
No reference solved value is used to fill a missing native decision variable.

Native solutions must pass reference feasibility at 1e-7; objective evaluation
must agree with native cost (1e-7 absolute, 1e-10 relative) and the independent
reference solve (1e-7 absolute, 1e-8 relative). All terminal quantities and derived
branch/valve/station losses are reported against the reference. Primal equality
is not a general OPF acceptance requirement because solutions may be nonunique.
Four deliberate port-power/DC-voltage corruptions must fail reference feasibility.

The qualification envelope here is case5 and case24, using the specified reference profile,
from three starts. Coupled PF solves, additional controls/cases, rebasing,
normalized cases, physical projections, LCC and directional losses remain future
work. The PowerIO preparation supplies reference station bounds/losses, DC
power scaling and pole conductance, and the canonical AC preparation. Tellegen
keeps an explicit checked adapter between typed physical quantities and native
coordinates. Ordinary AC-only preparation still refuses the equivalent owner.

PF equation diagnostics record the pinned PowerModels parser's PQ/PV bus-role
repair separately from source declarations. This is not a new coupled PF solver
or an implicit network edit. Case24's bus 123 exposes the difference; reference
reassignment without active generation remains unqualified and rejects.
