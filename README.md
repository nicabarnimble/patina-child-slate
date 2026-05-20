# patina-child-slate

Standalone Slate manager child for Patina.

Slate is a project-living build/refactor/fix workbench. This repository packages
Slate as an app-like Patina child with a strict WIT/WASI boundary:

- host owns authority;
- child owns Slate behavior;
- WIT owns the contract;
- declared toys/WASI capabilities own access.

## Build

Requires Rust, `cargo-component`, and the `wasm32-wasip1` target when prompted by
`cargo component`.

```bash
cargo component build --release
```

The component is written to:

```text
target/wasm32-wasip1/release/patina_ai_child_slate_manager.wasm
```

## CI

Tiered checks now run by stage:

- **Local pre-commit (Tier 0)**: `scripts/ci-tier0.sh` (`fmt` + `clippy`)
- **Push CI** (`.github/workflows/ci.yml`): Tier 1 + Tier 2 (`fmt`, `clippy`, `test`)
- **PR to main/master** (`.github/workflows/pr-main.yml`): Tier 1 + Tier 2 + Tier 3 (`fmt`, `clippy`, `test`, component build)

Enable the local pre-commit hook once per clone:

```bash
scripts/install-hooks.sh
```

## Versioning & releases

- This repo follows SemVer.
- Tag releases as `vX.Y.Z`.
- Tag pushes trigger Mother registry release asset publishing (`.wasm`, `child.toml`, sidecar hashes, and `checksums.txt`).
- See [`RELEASING.md`](RELEASING.md) and [`CHANGELOG.md`](CHANGELOG.md).

## Install into Patina

From this repository:

```bash
patina child install . \
  --wasm target/wasm32-wasip1/release/patina_ai_child_slate_manager.wasm \
  --force
```

For local development against a Patina checkout before the installed `patina`
binary has the latest command surface, run the same command through the Patina
source checkout:

```bash
cargo run --manifest-path /path/to/patina/Cargo.toml -- child install . \
  --wasm target/wasm32-wasip1/release/patina_ai_child_slate_manager.wasm \
  --force
```

Patina/Mother resolves the host project and mounts it into the child at `/project` for projectful Slate invocations. Slate receives `{"project":"/project"}` and should not need host paths or local filesystem scope edits in `child.toml`.

## Use

```bash
patina slate next
patina slate show <work-id>
patina slate check <work-id>
```

Child skill help is exposed through Mother when installed:

```bash
patina mother skills show slate-manager
patina mother skills help slate-manager slate-code
```

## Package contents

- `src/lib.rs` — Slate manager child behavior
- `child.toml` — Patina child manifest
- `wit/` — WIT world and package-style dependency contracts
- `wit-contract/` — Slate control interface contract copy
- `skills/` — child-owned agent skill packages

## License

MIT
