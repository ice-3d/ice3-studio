# Large Reference Asset Editing Design

> Document ID: ICE3-STUDIO-SPEC-011
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Summary

Studio needs to open and edit industrial models that already contain millions of
triangles without serializing their complete geometry into the Document response.
The current AI-authored scene path is appropriate for parametric geometry, but it is
not an efficient container for an imported production asset.

This design adds an immutable, project-local **Reference Asset** and a lightweight
semantic Document layer over it:

- the original source GLB, Prepared GLB, tileset, and equipment manifest remain
  immutable files in the project;
- the Document stores stable equipment nodes, transforms, material references,
  relations, and Agent-authored overlays;
- the Viewer loads the existing coarse-to-fine tileset and maps selection back to
  Document nodes;
- AI edits remain typed ChangeSets and do not mutate the referenced source asset.

This is not a second source of truth. The Reference Asset owns the imported geometry,
while the Document owns the project's editable semantics and every post-import
change.

## 2. Evidence

The real Console demonstration model has:

- 251,041,488-byte source GLB;
- 124,262,100-byte Prepared GLB;
- 1,761 source nodes and 204 named equipment records;
- 5,757,962 effective triangles;
- 259 tiled GLB files totaling about 380 MB.

Flattening that model into 5,065 inline custom-mesh Document nodes successfully
exported a 300 MB source GLB and a 169 MB preview, but the Viewer reached about
1.5 GB of browser memory and remained on `Loading project`. `document.get` also
failed once the response exceeded V8's maximum string length.

The failure is structural. It cannot be fixed by reducing RPC batch size.

## 3. Goals

- Open a 5M+ triangle reference asset without embedding its geometry in the
  Document.
- Preserve the source asset's visual appearance, world coordinates, materials,
  equipment selection identities, and coarse-to-fine LOD behavior.
- Let an Agent modify one piece of equipment without rebuilding or downloading the
  complete source scene.
- Keep `scene.context`, `scene.search`, and ChangeSet responses bounded regardless
  of reference asset size.
- Preserve compatibility with existing AI-authored parametric scenes.
- Make camera framing and selection rendering deterministic across rotations and
  viewport aspect ratios.
- Keep all project assets local by default.

## 4. Non-Goals

- Editing arbitrary imported triangle topology as structured component parts.
- Making the imported asset the canonical Document representation.
- Replacing Toolchain asset preparation, partitioning, or LOD generation.
- Modifying or rewriting the immutable source asset in place.
- Supporting remote asset URLs or credentials.
- Making native GPU or WebGPU execution default-on.
- Replacing existing GLB export for fully parametric projects.

## 5. Terminology

- **Reference Asset**: an immutable imported source asset plus its Prepared GLB,
  tileset, hashes, bounds, and equipment manifest.
- **Binding Node**: a lightweight Document node that refers to one equipment entry
  in a Reference Asset.
- **Overlay**: a normal Document node or typed patch that adds or changes behavior
  relative to the reference asset.
- **Reference Baseline**: the authored transform, material, and visibility recorded
  by the asset manifest before any Studio edit.

## 6. Project Storage

Reference assets live under the project directory:

```text
<project>/
  project.json
  revisions/
  changesets/
  references/
    <assetDigest>/
      asset.json
      source.glb
      prepared.glb
      tiled/tileset.json
      equipment.json
```

`asset.json` is an immutable descriptor written once during registration:

```text
ReferenceAssetDescriptor {
  assetId: string
  sourceDigest: string
  preparedDigest: string | null
  tilesetDigest: string | null
  bounds: { min: [number, number, number], max: [number, number, number] }
  stats: {
    nodes: number
    meshes: number
    triangles: number
    materials: number
    equipment: number
  }
  sourceFile: string
  preparedFile: string | null
  tilesetFile: string | null
  equipmentManifestFile: string
  importedAt: string
}
```

The descriptor remains metadata-only. It never embeds positions, indices, or
buffers.

Registration must reject paths outside the configured workspace, symlink escapes,
missing hashes, inconsistent manifest IDs, malformed GLBs, and unsupported units.
Files are copied into the project and made read-only at the application level.

## 7. Document Model

`NodeContent` gains:

```text
reference_instance {
  assetId: string
  selectionId: string
}
```

A project opening a 204-equipment reference therefore contains about 204 binding
nodes, not thousands of geometry nodes. Each binding node carries:

- stable Studio node ID;
- reference asset ID;
- stable imported selection ID;
- authored world transform;
- optional material override;
- visibility;
- metadata used by generic search and forms.

The source asset's original transform and material remain available in the
equipment manifest as the reference baseline. An authored transform is absolute
and starts at the baseline. Material and visibility overrides store only changed
values.

Reference nodes can coexist with parametric groups, instances, connections,
constraints, and Agent-authored geometry.

## 8. Public API

### 8.1 Register

`referenceAsset.register`

```text
{
  sourcePath: string
  preparedPath: string | null
  tilesetPath: string | null
  equipmentManifestPath: string
}
```

This is an approval-capability method. Paths are relative to the current workspace.
The method validates and hashes the files before creating a descriptor. It returns:

```text
ReferenceAssetRegistration {
  assetId: string
  descriptor: ReferenceAssetDescriptor
}
```

Registration is idempotent for identical content hashes.

### 8.2 Get

`referenceAsset.get`

```text
{ assetId: string }
```

Returns the descriptor plus:

```text
{
  tilesetUrl: string | null
  preparedPreviewUrl: string | null
  equipmentManifestUrl: string
}
```

URLs are Studio-server routes scoped to the project and asset digest. They never
expose filesystem paths.

### 8.3 List Bindings

`referenceAsset.bindings`

```text
{
  assetId: string
  cursor: string | null
  limit: number
}
```

Returns at most 100 binding nodes with stable ordering. This supports large
equipment catalogues without calling `document.get`.

### 8.4 Existing Methods

- `scene.context` includes reference asset counts and memory-bounded statistics.
- `scene.search` searches binding metadata and authored overlays.
- `scene.query` may return a binding without geometry payloads.
- `evaluation.stats` aggregates imported baseline statistics and authored overlays.
- `document.get` returns semantic nodes only.
- `changeset.impact` classifies reference binding changes without serializing the
  source geometry.

## 9. Viewer Data Flow

```text
scene.context + referenceAsset.get
                |
                v
        tile manifest + binding IDs
                |
                v
       coarse-to-fine tileset loader
                |
                v
       visual asset + stable selection map
                |
                v
      Document transforms/material overlays
```

The Viewer must not wait for `document.get` before displaying the reference asset.
It first renders the coarsest usable tile, then refines it in the background.

Selection IDs from the tile metadata map to binding nodes through the equipment
manifest. Unmapped imported nodes remain visible but are not selectable as semantic
equipment.

Asset replacement retains the previous ready asset until the replacement's
descriptor and coarse tile are ready. A failed replacement does not hide a valid
baseline.

## 10. Editing Semantics

- Transform, visibility, metadata, and material overrides update a binding node.
- Material overrides affect matching imported material roles or selection IDs.
- Deleting a binding hides or removes that imported equipment without mutating the
  source asset.
- Adding a parametric instance or connection creates a normal Document node.
- Topology edits inside imported mesh data are unsupported in this phase.
- Reset-to-baseline removes the authored override.
- Undo and redo operate on binding and overlay nodes through normal Document
  history.

## 11. Rendering And Camera Requirements

Camera framing must use the actual viewport aspect ratio and a safety margin. The
projection of all eight world-bounds corners must remain inside 90% of normalized
device coordinates for both landscape and portrait viewports.

The Viewer uses a large-scene clip range with a near plane no smaller than the
smallest supported inspection distance. The chosen range must preserve the full
asset bounds and must be regression-tested against the project bounds.

Selection tint must use one of:

- a same-pass selection attribute; or
- a depth-safe overlay with equivalent negative bias on every selection path,
  including batched semantic selections.

Ground, service-yard, road, and water surfaces must have distinct vertical levels
and may not be coplanar. The acceptance scene must remain stable during continuous
rotation and zoom.

## 12. Failure And Security Rules

- Invalid descriptor, hash mismatch, or unsupported GLB returns a structured
  diagnostic and leaves the current project unchanged.
- A missing tile falls back to Prepared GLB, then reports a bounded diagnostic if
  neither asset is usable.
- Reference routes reject traversal, symlinks outside the project, ranged writes,
  and mutable HTTP methods.
- The response does not expose absolute filesystem paths.
- Asset registration requires approval in `review-required` mode.
- Asset bytes never enter a normal JSON-RPC response.

## 13. Export And Replacement

For an unmodified reference project, `export.sourceGlb` streams the registered
source file without rebuilding it.

Composing reference geometry and Document overlays into a new monolithic GLB is not
part of this phase. If authored reference overrides are present,
`export.sourceGlb` returns `ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED` rather than
silently exporting only the baseline. The Viewer and approved preview still apply
the overrides in real time.

Registering replacement bytes writes to a temporary directory, verifies all hashes
and metadata, and atomically publishes a new digest directory. Revisions that
reference the previous digest remain readable.

## 14. Performance Requirements

On the existing large wastewater reference:

- committed Document response is under 2 MB;
- `scene.context` response is under 16 KB;
- binding pages are under 128 KB;
- the shell becomes interactive before the coarse tile finishes;
- opening the project does not allocate the full source GLB in the browser;
- the first visible coarse scene has a stable selection map.

Hardware-dependent frame time remains measured rather than hard-coded.

## 15. Testing

- Path traversal, symlink escape, bad digest, malformed descriptor, and missing
  manifest tests.
- Reference registration idempotency.
- Binding-node serialization, generated schema, and TypeScript declarations.
- `scene.context`, `scene.search`, and binding pagination stay bounded.
- `document.get` for the large reference remains under the configured response
  budget.
- Transform, material, visibility, reset, undo, and redo through ChangeSets.
- Impact classification for local binding edits and overlay/node additions.
- Selection mapping from imported IDs to Studio node IDs.
- Tileset loading, coarse-first display, fallback to Prepared GLB, and retained
  previous asset on failure.
- Camera projection tests for portrait and landscape viewports.
- Repeated selected-scene rotation and zoom do not switch large surface colors
  because of depth conflict.

## 16. Acceptance Criteria

- The real 204-equipment reference asset opens in Studio with its original visual
  appearance and coarse-to-fine loading.
- The Document contains semantic binding nodes only and remains below 2 MB.
- No `Loading project` stall or multi-gigabyte Document response occurs.
- The complete asset bounds remain visible at initial framing for portrait and
  landscape viewports.
- Selected ground, road, water, and building surfaces remain stable during rotation
  and zoom.
- An Agent can update one binding's transform or material through a local ChangeSet
  without rebuilding unrelated reference geometry.
- Existing parametric projects and exports continue to pass the full verification
  suite.

## 17. Implementation Evidence

The real-asset acceptance run registered the 251,041,488-byte source GLB, the
124,262,100-byte Prepared GLB, the tiled asset, and the 204-entry equipment manifest.
It committed 204 semantic binding nodes and produced:

- 70,984-byte `document.get` result;
- 731-byte `scene.context` result;
- 41,545-byte first binding page;
- about 330 ms from Viewer navigation to the first visible coarse tile;
- 5,757,962 effective triangles and 1,632 unique meshes;
- safe portrait and landscape bounds with no camera clipping;
- zero selected-surface changes across stable screenshot frames;
- a local transform/material edit with zero generated reference geometry.

The editable responses remain bounded because the copied source, Prepared, and tiled
files are served outside JSON-RPC. The reproducible report and screenshot are written
to `output/large-reference-acceptance.json`,
`output/large-reference-acceptance.png`, and the rotated/zoomed selection view
`output/large-reference-acceptance-selected.png`.
