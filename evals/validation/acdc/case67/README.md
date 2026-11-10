# Case67 native OPF qualification with numerical angle references

```sh
python3 evals/validation/acdc/run.py /tmp/tellegen-case67 --case case67
```

The fixture explicitly selects `LowestBusIdPerIsland`; the public constructor's
`Declared` default still refuses this source's uncovered AC island. The run
checks original-source OPF equations, derivatives, three native starts and the
complete independent reference model. Reference PF remains unqualified.

`case67.m` is byte-exact [PGLib-HVDC at f928f95c](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case67.m):
30,996 bytes, 329 lines, SHA-256
`1f7637a093a5c9968f5571b323875a57f60f319a665600e0b5e11d9463c9cbf1`.
The source retains attribution and is CC-BY-4.0 under [the upstream license](../LICENSE).
It has 67 AC buses, 20 generators, 102 AC branches, nine DC buses,
11 DC branches and nine VSCs. It uses canonical `busdc/branchdc/convdc` names.
The checked identity map uses source IDs, never solver indices.

## Numerical reference policy

PowerIO model commit `552fc413` introduces explicit intent, persisted in the
coupled instance/result. All declared references are retained. For each active
AC island without one, the selected policy anchors its lowest source bus ID
at zero radians, recording the component and numerical origin. DC edges do not
join AC phases. Source bus roles and PF generation/voltage controls are unchanged.
Ordinary AC-only guards remain; added numerical OPF anchors do not qualify PF.

Case67 has buses 1–66 anchored by declared bus 1, and a singleton AC island at
bus 67 with its generator and converter 9. Bus 67 remains PV in source data.
The reference model leaves this component's common phase free. Native
preparation adds only `reference_angle[67] = 0`; policy and anchor records enter
the version-3 model manifest/fingerprint.

Explicit alignment shifts `va[67]`, `vaf[9]` and `vac[9]` together by
-0.021358541264414423 radians. The operation returns the transformed copy and
shift record; supplied starts are otherwise exact. It refuses transformed
angles outside existing bounds, without clipping/wrapping. The independent
Julia checker derives the expected anchor from original topology and checks
the complete aligned oracle point in the unchanged reference model: maximum
violation 6.822e-13 and unchanged objective. Raw oracle angles are retained in
`reference.json`; electrical differences below use the recorded alignment.
The earlier +/-0.05-radian common-phase probes remain recorded as well.

## Results, 2026-11-10

PowerModelsACDC stays pinned to `9ada4082` (0.10.1), with unchanged solver options
and tolerances. The aligned OPF oracle satisfies 989 native rows and 264 bounds
at 5.447e-13 maximum violation, objective 122253.02388749266.

| Native start | Iterations | Objective | Native violation | Reference violation |
| --- | ---: | ---: | ---: | ---: |
| Reference | 14 | 122253.02388753793 | 6.739e-11 | 6.739e-11 |
| Declared | 24 | 122253.02388749142 | 2.860e-13 | 4.548e-13 |
| Flat | 69 | 122253.02388753791 | 6.734e-11 | 6.734e-11 |

All 739 native coordinates map into 859 reference variables, with 120
fixed-demand/unused VSC auxiliaries reconstructed from declarations. Maximum
objective difference is 4.531e-8, electrical difference 6.454e-8 and native KKT
error 6.558e-9. Three OPF derivative points pass 90 summaries at both steps;
maximum scaled gradient/Jacobian/Hessian errors are
8.005e-11 / 5.425e-10 / 4.174e-8, below the unchanged 1e-6 threshold.

All five negative controls fail reference acceptance: four port-power/DC-voltage
corruptions, plus shifting only bus 67 (violation 3.096). Rust tests cover bound
refusal, exact supplied starts, policy-sensitive fingerprints, preserved source
roles, strict-default/PF refusal and physical IR round trips under the selected
policy. The old preparation failure artifact has been replaced by `summary.json`.

Reference PF still returns `LOCALLY_INFEASIBLE`, violation 0.5991975989165781.
Its point is excluded from acceptance. This local solver outcome is not an
infeasibility proof. No PowerModelsACDC bug was found. Qualification covers this
source-coordinate VSC OPF profile, not coupled PF solving, physical rebasing,
global optimality, arbitrary phase rotations within finite bounds, or duals.
