# ravel-phext verification suite

- `harness.py` — the v1 verification harness: quad-for-quad roundtrip
  against a reference projector transcribed from ravel's Rust sources,
  read-back fidelity, byte-identical memory logs, fixed queries, and
  the HTTP phase. Run via `run-local.sh`.
- `wavelets_test.py` — unit tests as executable documentation of
  Wavelets 1–3 (2026-11-01 → 2026-11-21) in ravel's vocabulary: one
  soul, one transcript per wavelet, the era's claims as annotations,
  its standing facts as the optmem log. Every asserted fact cites its
  source in the dated notes. Run: `python3 -m unittest wavelets_test -v`.
- `measure.py` — backend-self timings for the v1 comparison table.
