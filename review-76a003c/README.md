# Transmission results screenshots for PR 300

Captured from source commit `76a003c5431d9bbd898be67908912f7c7a4180cb` on 2026-11-05,
using the production build and headless Chromium. These assets stay on the PR asset branch.

All views use the public TAMU ACTIVSg200 MATPOWER case. Geographic coordinates come from its
PowerWorld aux file. The coordinate-free comparison omits that sidecar and shows synthetic
positions followed by the stress-based substation schematic.

Images 4 and 10 show DC OPF and AC power-flow results respectively. Image 11 shows AC results
on a phone. Utilization warnings and at-limit operation are amber; violations are red. Voltage
uses a fixed 0.90–1.10 pu scale with source limits determining actual violations.

Desktop viewport: 1440 × 900 CSS pixels. Phone: 390 × 844 with emulated touch input. The
coordinate-free pair uses the same 440 × 400 crop. These are direct browser screenshots with
reduced motion; capture.json records the parameters. Physical phone testing remains separate.
