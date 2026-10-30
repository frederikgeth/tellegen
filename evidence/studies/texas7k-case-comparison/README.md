# Texas7k catalogue comparison screenshots

Captured 30 October 2026 from the same local frontend, using two local catalogues:

- Integrated: the compact main energized island of p1uhs0_1247 currently proposed by PR #169, 22,076 modeled buses and 15,299 loads.
- Per-feeder prototype: six independent constituents from the same PowerIO 0.11.7 preparation. Screenshot example: p1udt1566, 6,002 buses and 4,276 loads.

Both discovery screenshots start from Texas transmission. Distribution power-flow screenshots show the nominal snapshot, with no load edits. Each map is fitted to its selected case, so the map extents differ. These are UI illustrations, not a controlled performance benchmark. Transmission and distribution remain separate calculations.

The integrated case retains shared upstream substation equipment. The per-feeder snapshots fix head/shared-interface voltages to the parent nominal solution; later edits do not propagate into the station or neighboring feeders. Both omit the originally unsupplied parts and retain all 15,299 parent loads across the distribution entries.

This branch stores screenshots for the PR discussion. The per-feeder implementation remains a local experiment based on the refreshed PR head.
