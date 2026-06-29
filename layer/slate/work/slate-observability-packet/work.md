# Add Slate observability packet and metrics

## User problem

The user loses the plot as AI agents build more freely. Slate exists to replace the older Patina spec workflow as a standalone WASI/WASM child, but it needs enough product-closure structure and observability for agents to answer current state from evidence.

## Product outcome

Slate should stand alone as the work authority. An agent should be able to ask Slate for a packet and answer:

- what work is active,
- why it exists,
- what is in scope,
- what proof remains,
- what has changed,
- what is blocked,
- what cleanup is needed,
- what the next safe action is.

## Scope

- Enrich or stabilize `packet-work` as the canonical agent state endpoint.
- Add metrics for operation, lifecycle, proof, and gate state.
- Add host CLI parity for missing read-only Slate operations.
- Use the Grafana-Labs Rust observer as the proving harness.

## Non-goals

- Do not redesign Patina Mother.
- Do not remove the legacy spec bridge in this slice.
- Do not build the full Grafana dashboard inside this repo; the dashboard lives in the Grafana-Labs project.

## Stop condition

Stop when an agent can answer “what is going on?” from `packet-work`, `history-work`, and emitted metrics without relying on conversation memory.
