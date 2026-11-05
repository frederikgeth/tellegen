# Transmission operating limits for PR 300

Captured from source commit `1f3ea5426fbfff88a380a62ce69e4c9f7bcce0a7` on 2026-11-05,
using the production build. These assets stay on the PR asset branch.

Images 1–12 use the public TAMU ACTIVSg200 MATPOWER case. Geographic coordinates come from its
PowerWorld aux file; the coordinate-free comparison omits that sidecar and uses the same
440 × 400 crop before and after the stress-based layout. The main captures use installed Chrome
in headless mode at 1440 × 900 desktop and 390 × 844 emulated touch viewports.

Image 4 shows DC operating limits; 10 and 11 show AC voltage and utilization on desktop and
phone. Image 12 changes the display warning to 65%, revealing amber gauges without changing the
solve. Image 13 shows the two-bus station fixture with 1.0200–1.0800 pu and the worst bus retained.
Image 14 shows a separate DC fixture at 100% in Playwright Chromium touch emulation: amber and
zero overloads. See `capture.json` for viewport and capture parameters.
