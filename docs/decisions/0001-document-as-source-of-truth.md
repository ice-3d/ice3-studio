# 0001: Document and Operation History Are the Source of Truth

> Document ID: ICE3-STUDIO-ADR-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-10-30

## Context

An AI-native modeling system needs deterministic change review, retries, undo, and recovery.
Treating an evaluated mutable scene graph as the only source would make semantic changes hard
to distinguish from transient geometry and renderer state.

## Decision

Persist a serializable `Document` and its append-only transaction history. Treat evaluated
meshes, bounds, diagnostics, source GLB, and runtime preview as derived values.

Every accepted batch is one atomic revision. Stable IDs remain independent of names and
revision numbers. Geometry can be recomputed from document content.

## Consequences

- AI operations can be inspected, diffed, retried, undone, and replayed.
- Failed transactions leave the previous snapshot unchanged.
- Cache invalidation and incremental evaluation have explicit document dependencies.
- The implementation must maintain schema migrations and deterministic serialization.
- Large derived assets cannot be the sole record of a model.
