# Screenshots for eigenergy/tellegen#185

Images referenced from the pull request description; not part of the code.

`norwood/`: the SAPN Norwood 10-feeder model (25,456 buses),
[`output/SAPN/norwood--p95-lv-proxy-transfer.bmopf.json`](https://github.com/frederikgeth/BMOPFDraftData/blob/main/output/SAPN/norwood--p95-lv-proxy-transfer.bmopf.json)
in [BMOPFDraftData](https://github.com/frederikgeth/BMOPFDraftData/tree/main/output/SAPN). Data
licensing and attribution are in that folder (`ATTRIBUTION.md`, `LICENSING.md`). Basemap © CARTO
© OpenStreetMap contributors.

`norwood/13-*`, `norwood/14-*` add 8 synthetic 70 kW batteries to a scratch copy of the Norwood model
(not part of the published data) to show inverter-based resources.

`oracle/`: Tellegen's own test case `crates/tellegen/tests/data/mc_pf/oracle_inputs/pf_1ph_line.json`.

`schematic-symbols.png`, `schematic-zones.png`: the French feeder `75_MVFeeder0100` from
[Representative French Power Grids](https://github.com/frederikgeth/Representative_French_Power_Grids).
