# ChangeSet Review Contract

> Document ID: ICE3-STUDIO-REF-CHANGESET-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

A ChangeSet is a persistent, reviewable proposal derived from an immutable project revision. It
does not modify the committed Document until explicitly approved and committed.

## Lifecycle

| Status | Meaning |
|---|---|
| `draft` | Accepting Operations; the candidate is stale or not evaluated |
| `evaluating` | Candidate evaluation is in progress |
| `ready` | Candidate passed validation and has a digest |
| `blocked` | Candidate validation or preview failed |
| `stale` | The project revision advanced beyond the base revision |
| `committed` | Operations were committed as one new revision |
| `discarded` | Proposal was abandoned without modifying the project |

Adding or removing an entry requires the current `expectedVersion`, increments `version`, and
invalidates the candidate digest and diagnostics.

## API

The following public methods implement the review lifecycle:

- `changeset.create`
- `changeset.list`
- `changeset.get`
- `changeset.addOperations`
- `changeset.removeOperations`
- `changeset.evaluate`
- `changeset.getCandidate`
- `changeset.commit`
- `changeset.discard`

`changeset.evaluate` returns the ordered diff, diagnostics, and a SHA-256 candidate digest.
`includeCandidate` controls whether the derived Document is included in that response.

`changeset.commit` requires:

- matching `expectedVersion`;
- matching current base revision;
- `approvedDigest` equal to a fresh evaluation;
- approval capability;
- a unique idempotency key.

## Persistence

```text
project/
├── changesets/<change-set-id>.json
├── cache/candidates/<digest>-source.glb
└── cache/preview/<digest>.glb
```

ChangeSet files are canonical JSON written through a temporary sibling and atomic rename. Candidate
previews are content-addressed. ChangeSets survive process restart. A malformed ChangeSet is
isolated and reported without preventing the current revision from loading.

## Permissions

`review-required` is the default mode. The Agent proposal token may read, create and edit
ChangeSets, evaluate candidates, and request candidate previews. It cannot commit, approve, discard,
or call `document.apply`.

The human approval capability is delivered by the single-use loopback bootstrap endpoint and stored
in an `HttpOnly`, `SameSite=Strict` cookie. It may commit or discard proposals. It is not returned
to page scripts or model prompts.

`autonomous` mode must be enabled explicitly by host configuration. It grants its trusted writer
token direct transaction and ChangeSet commit rights.

## Events

The authenticated WebSocket publishes:

- `changeset.created`
- `changeset.updated`
- `changeset.evaluated`
- `changeset.committed`
- `changeset.discarded`
- `revision.committed`

Committing a ChangeSet publishes `changeset.committed` before `revision.committed`.

## Recovery

- A failed evaluation leaves the committed revision unchanged.
- A failed candidate preview retains previously valid candidate files.
- A stale base marks the ChangeSet stale and rejects commit with
  `ICE3_STUDIO_REVISION_CONFLICT`.
- A candidate modified after review fails digest verification and is reported as
  `ICE3_STUDIO_OPERATION_INVALID`.
- Duplicate commits with the same idempotency key return the original result.
