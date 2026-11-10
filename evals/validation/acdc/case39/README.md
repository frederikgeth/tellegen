# Case39 native OPF qualification

```sh
python3 evals/validation/acdc/run.py /tmp/tellegen-case39 --case case39
```

`case39_10_he.m` is byte-exact [PGLib-HVDC at f928f95c](https://github.com/power-grid-lib/pglib-opf-hvdc/blob/f928f95cde06d035554d92854f03950e074850e0/case39_10_he.m):
23,466 bytes, 364 lines, SHA-256
`678a3c76c36663aef7938117d77e1a9c66bd466ebcbe98405530e3a52b6f212d`.
The source retains attribution and is CC-BY-4.0 under [the upstream license](../LICENSE).
It contains 39 AC buses, 10 generators, 46 AC branches, 10 DC buses, 12 DC
branches and 10 VSCs. `identity-map.json` records source bus IDs and equipment
row identities, checked against the independent source parser's endpoints.

## Results, 2026-11-10

PowerIO remains at `98d5c88f`, PowerModelsACDC at `9ada4082` (0.10.1), with the
shared pinned Julia environment and unchanged tolerances. The independently
solved OPF point satisfies all 555 native rows and 233 bounds with maximum
violation 4.564e-10; the objective is 138063.51729486606.

| Native start | Iterations | Objective | Native violation | Reference violation |
| --- | ---: | ---: | ---: | ---: |
| Reference | 18 | 138063.51729275694 | 5.508e-12 | 5.508e-12 |
| Declared | 147 | 138063.51729275903 | 1.494e-13 | 6.595e-14 |
| Flat | 95 | 138063.5172927591 | 1.643e-13 | 7.328e-14 |

All 456 native coordinates map into 529 reference variables; the remaining
73 fixed-demand/unused VSC auxiliaries are reconstructed from declarations.
The largest objective difference is 2.110e-6, within the existing relative
acceptance tolerance. Maximum electrical difference is 2.114e-6 in source pu
or radian coordinates. Four port-power/DC-voltage corruptions are detected.
Typed physical results round-trip through PowerIO IR and remain feasible.

Three OPF derivative points (reference and two off-feasible perturbations)
pass all 90 summaries at both steps. Maximum scaled gradient/Jacobian/Hessian
errors are 7.646e-11 / 3.914e-10 / 3.992e-8, below the unchanged 1e-6 threshold.

Reference PF returns `LOCALLY_INFEASIBLE`, maximum violation 0.4694775713833841.
The failed PF point is not used for native equation or derivative acceptance.
This is a local solver outcome, not proof of infeasibility and not a reader
failure. The source's converter controls remain unchanged; OPF frees dispatch.

`reference.json` retains the accepted OPF point, versions, hashes and explicit
failed-PF status, omitting starts, duplicate equipment and the failed PF point.
`summary.json` records the full live run. Larger reports and physical IR outputs
are reproducible outside the repository. This is local OPF qualification, not
coupled PF solving, physical rebasing, global optimality or dual qualification.
