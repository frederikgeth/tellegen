# Follow-up: saturation-aware coupled PF

We probably want to add proper saturation later. Track it as a separate control
policy and qualification gate; successful unsaturated PF is not evidence that
rating-constrained operation is possible.

The current profiles solve the declared controls and report generator/converter
limit violations afterwards. They do not clip a solved point, switch controls,
redistribute a saturated participant's share or enforce OPF operating ratings.
That behavior remains explicit in result manifests.

Before implementation, define:

- Which limits trigger a control transition: generator P/Q, converter PCC or
  valve P/Q, valve RMS current and voltage. Preserve their distinct power
  locations, units and physical versus reference-compatible current policies.
- How the remaining controllers share P/Q when a participant saturates, and
  which voltage or droop equation yields to which binding limit. A clamped
  output must be part of a new solved equation system, not a post-solve edit.
- What happens when every voltage-supporting controller in a DC island
  saturates, or an AC reference cannot balance the island. Return an explicit
  control/feasibility outcome rather than silently inventing a new controller.
- An active-set or complementarity formulation with deterministic transition
  rules, convergence reporting and handling of boundary ties/cycles. Record
  requested controls, active controls, binding limits and residuals in typed
  results; version the policy in IR and preparation fingerprints.

Qualification should cover entering/leaving saturation, simultaneous limits,
unequal ratings, mixed generator/converter voltage groups, droop-only DC islands,
outages and disconnected components. Check conservation after every accepted
control transition, compare independent formulations, and use one-sided or
active-set derivative checks at nonsmooth boundaries. Include cases with no
feasible saturated operating point. The unsaturated interior must reproduce the
current profiles without changing their existing semantics.

This is planned follow-up, not part of the first native DC-power-droop profile.
