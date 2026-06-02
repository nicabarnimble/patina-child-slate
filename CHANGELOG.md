# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project uses Semantic Versioning.

## [Unreleased]

## [0.3.0] - 2026-06-02

### Added
- Slate work items now get a sibling `work.md` narrative body and prompt/handoff outputs include narrative summaries, linked context docs, and dependency direction.

### Changed
- Split Slate model, store, runtime, control protocol, work body, narrative, dependency graph, lifecycle, work fields, tests, commands, and views into focused modules.
- Replaced internal Slate work status/kind strings with TOML-compatible enums.

### Fixed
- Dispatch bridge commands prefer the Slate work store before legacy SPEC.md fallback.
- Work dependencies reconcile `blocked_by`/`blocks` bidirectionally and unblock stale blocked work when blockers complete.

## [0.2.1] - 2026-05-20

### Fixed
- Removed Slate child host-path remapping fallback that produced invalid `/input/<host-absolute-path>` paths.
- Removed broad filesystem manifest scope so Slate relies on Patina/Mother runtime project mounting.
- Documented the `/project` runtime mount contract for projectful Slate invocations.

## [0.2.0] - 2026-05-12

### Added
- Tiered CI model across local pre-commit, push, and PR-to-main stages.
- Local git hook installer and Tier 0 pre-commit checks (`fmt` + `clippy`).
- Tag-driven release workflow that publishes wasm + sha256 artifacts.
- Slate schema/discoverability surface for mutable fields and lifecycle gates.
- First-class `set-work` editing operations: set/add/remove/update.
- Editable `human_request`, `allium_anchors`, and `release_contract` fields through Slate APIs.
- Explicit `activate-work` lifecycle transition.
- Generic, ecosystem-aware release contract with monorepo release units.
- Initial Slate work tracking entries under `layer/slate/work/`.

### Changed
- Ready-gate validation now returns actionable missing-gate details.
- Slate archive/recovery tags were created for completed work items.

## [0.1.0] - 2026-05-12

### Added
- Initial standalone `slate-manager` WIT/WASI child extraction from Patina monorepo.
