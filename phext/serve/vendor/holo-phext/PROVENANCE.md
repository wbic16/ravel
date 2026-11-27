# Vendored: holo-phext 0.2.5

Provenance record for this vendored copy.

- Upstream repository: https://github.com/wbic16/holo-phext
- Upstream revision: bafed5f36ebfad92f1011fe5c2bd8bdd3c3a9511
- Version: 0.2.5 (per the upstream `Cargo.toml` at that revision)
- License: MIT (per the upstream `Cargo.toml` at that revision)
- Vendored: 2026-11-27, leg D of the ravel phext graph-layer rewrite
  (ashfall convergence), EE 1058+.

Why vendored: holo-phext is not published on crates.io (checked
2026-11-27: the crates.io sparse index has no `holo-phext` entry), and
ashfall's vendorability rule flags git-sourced dependencies. This copy
is byte-for-byte the upstream source at the pinned revision
(`src/{lib,core,index,compat}.rs` + `Cargo.toml`, unmodified), so the
ravel tree is self-contained and vendorable end to end. Any future
change to these files is a divergence from upstream and must be
recorded in the repo-root `PATCHES.md`.
