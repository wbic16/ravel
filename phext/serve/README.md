# phext-serve

Ravel's serving machinery, relocated from phext-lattice in the
2026-11-26 de-slop (census + rationale: `../DESLOP.md`).

Build:

```
cargo build --release   # phext-serve (the store host)
cargo build             # debug phext-sparql (the backend's default evaluator path)
cargo test              # the evaluator's 26 unit tests, moved with it
```

Run the store host: `phext-serve <port>` with cwd = data dir
(one `<soul>.phext` + WAL per soul). The ravel backend selects it
with `RAVEL_TRANSPORT=holo`; binary paths default to this crate's
own build output (`RAVEL_HOLO_BIN` / `PHEXT_SPARQL_BIN` override).

Smokes (the server's own, moved with it):

```
scripts/phext_serve_smoke.sh            # 30 checks: endpoints, restart persistence
scripts/phext_serve_checkpoint_smoke.sh # 15 checks: WAL growth, /checkpoint, SIGTERM
```
