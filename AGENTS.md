# ICE3 Studio Repository Rules

> Document ID: ICE3-STUDIO-POL-001
>
> Status: Active
>
> Owner: ICE3 Studio maintainers

## Scope

- This repository is reserved for the ICE3 authoring workspace.
- Do not invent a public API, implementation architecture, or delivery workflow before an approved design decision exists.
- When implementation begins, tooling and authoring code must remain outside the runtime and application repositories unless an approved contract states otherwise.

## Delivery

- Develop on a feature branch and merge to `main` only after the applicable verification gate passes.
- For future executable work, use the Node.js version pinned in that implementation's `.nvmrc`; the ICE3 family baseline is Node.js 24.22.1 LTS.
- Every behavioral change follows RED-GREEN-REFACTOR.

## Commit identity

- Every Git author and committer name in this repository must be `damoqiongqiu`.
- Verify the configured and recorded names before committing, merging, or pushing.

## Naming and legal boundary

- Do not use competitor, vendor, or third-party product names in documents, code, comments, identifiers, links, examples, diagnostics, or commits.
- Describe comparisons only with neutral category names.
- Do not link to external materials that expose prohibited names. Keep any necessary research outside product repositories and subject it to legal review.
- Audit names before every commit and release.

## Customer asset protection

- Real customer model assets remain in a controlled local directory outside every repository.
- Original models and derived assets must not enter source, tests, examples, documentation, screenshots, logs, build output, packages, or Git history.
- Derived assets include converted models, simplified models, previews, thumbnails, extracted textures, and other identifiable content.
- Do not stage, commit, push, or upload customer assets or derivative data to any remote service.
- Tests, demos, and public builds use synthetic, sanitized, or explicitly redistributable assets.
- Before completing asset-related work, inspect Git status, tracked files, ignore rules, and staged content.

## Cross-repository contracts

- `ice3-doc` is the single source of truth for formats, metrics, budgets, compatibility rules, shared diagnostics, and other cross-project contracts.
- This repository links to canonical contracts instead of independently maintaining complete copies.
- A contract change requires checking every consumer in the ICE3 family and passing the shared validation checks.

## Verification

No executable verification gate exists while this repository remains a placeholder. Before merging documentation-only changes, confirm that tracked placeholder claims remain accurate and the worktree contains only intended files.

## Related Documents

- `ICE3-STUDIO-RM-001` — [Repository README](README.md)
