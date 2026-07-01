---
type: belief
id: mother-owns-child-project-mounts
persona: architect
facets: [mct, children, wasi, filesystem, mother, least-authority]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-20
revised: 2026-05-20
---

# mother-owns-child-project-mounts

Mother and the child runner should own host-to-guest project mounting; WASM children should receive guest-visible project roots such as /project, not host absolute paths.

## Statement

Mother and the child runner should own host-to-guest project mounting; WASM children should receive guest-visible project roots such as /project, not host absolute paths.

## Evidence

- Session [[session-20260512-170906]] identified and fixed Slate's invalid /input/<host-absolute-path> remap in [[commit-aadfba8]] and documented runtime project mounting in [[commit-f83da32]].

## Supports

- [[safety-boundaries]]
- [[adapter-pattern]]

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[commit-aadfba8]] removed child-side host-path remapping fallback.
- [[commit-f83da32]] removed broad filesystem scope and documented `/project` runtime mounting.
- [[commit-ccf5ead]] released the corrected mount contract as `v0.2.1`.

## Revision Log

- 2026-05-20: Created — metrics computed by `patina scrape`
