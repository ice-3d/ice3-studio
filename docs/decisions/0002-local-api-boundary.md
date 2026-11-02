# 0002: The Local API Is the Product Boundary

> Document ID: ICE3-STUDIO-ADR-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-10-30

## Context

AI clients, automation, and the Viewer need the same modeling capabilities. A UI-only path
would make headless operation impossible and let rendering concerns leak into the modeling
kernel. A remote service would add account and network dependencies before the local contract
is stable.

## Decision

Expose a loopback-only JSON-RPC API backed by the Rust Studio Server. External AI performs
natural-language interpretation and submits typed operations through this API. The Web Viewer
is a thin client with no private mutation route.

Mutating requests require a short-lived session token. Project paths and configured Toolchain
executables are host-controlled. Studio owns no AI model credentials or agent policy.

## Consequences

- Headless workflows and the Viewer exercise the same contract.
- API schemas, operations, diagnostics, and events must remain versioned and documented.
- Loopback and workspace restrictions limit local exposure but do not provide multi-user
  authorization.
- A future remote deployment requires a separate security and tenancy design rather than
  exposing this local boundary directly.
