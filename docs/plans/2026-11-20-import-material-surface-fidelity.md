# 导入材质表面保真实施计划

> Document ID: ICE3-STUDIO-PLAN-031
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This repository defaults to Native execution; do not spawn or delegate to subagents unless the user explicitly authorizes them.

**Goal:** 让 glTF/GLB 的材质因子、四类 PBR 贴图、源 UV、transform 和 sampler 经 Studio 导入、Prepare、Import/Build/Partition 全链路保真，并随输出发布安全的纹理资源。

**Architecture:** 在 `studio-import` 与 `studio-assets` 之间建立格式无关的 Surface、texture binding 和内容寻址资源契约。glTF 适配器把外部/data URI/内嵌图片归一化为受限资源，唯一 canonical writer 生成相对路径 Prepared GLB，CLI 以 bundle 事务发布 GLB 和纹理目录。

**Tech Stack:** Rust 2024、serde/bincode、serde_json、sha2、base64、percent-encoding、tempfile、glTF 2.0、现有 Studio Core/Assets/Import/Pipeline crate。

**Spec:** `docs/specs/2026-11-20-import-material-surface-fidelity-design.md`

## Global Constraints

- 默认 Native 执行；不派生子 Agent，除非用户明确允许。
- 第一阶段只支持 glTF 2.0 JSON/GLB；OBJ/MTL、FBX、USD、STEP、IFC、JT 纹理不在本计划内。
- 只支持并输出 `TEXCOORD_0`；`TEXCOORD_1+`、sparse UV 和其它 UV 编码产生确定性 warning，不得贴错槽。
- 单个图片最多 `64 MiB`，图片总量最多 `256 MiB`，图片数量最多 `4096`，整个 `SourceScene` 仍受 `max_memory_bytes` 约束。
- 只接受 PNG、JPEG、WebP；文件名只使用 lowercase SHA-256 和规范化扩展名。
- 所有纹理 URI 必须是相对 GLB 的路径；拒绝绝对路径、协议 URL、`..`、根组件和符号链接逃逸。
- `studio-assets::prepare` 是唯一 Prepared GLB writer；不得引入外部转换器或 Node runtime。
- 无新 surface 的既有 canonical 输出保持 byte-identical。
- Worker 协议升级到版本 2；图片字节作为 bincode 二进制载荷传输。
- 每个任务先写失败测试、实际观察失败、再写最小实现、观察通过并提交。
- Git author 和 committer 必须为 `damoqiongqiu <damoqiongqiu@126.com>`。
- 文档、代码、commit 和诊断不得出现竞品、厂商或第三方产品名称。

## Review Focus

- 同一图片绑定到两个不同 sampler 时，必须生成两个 texture index；测试在 Task 7。
- 百分号编码路径、符号链接和目录逃逸不能绕过 confined reader；测试在 Task 3。
- 强制覆盖时如果纹理目录或 report 发布失败，旧 GLB、旧纹理和旧 report 必须全部恢复；测试在 Task 11。
- 多 UV 或无法解码的 UV 不得被错误绑定到 `TEXCOORD_0`；测试在 Task 4 和 Task 5。
- 无贴图、无新 surface 的历史路径必须保持 byte-identical，并继续在无 UV 时使用盒投影；测试在 Task 6 和 Task 7。

---

### Task 1: Source Surface、资源和 UV 契约

**Files:**
- Create: `crates/studio-assets/src/surface.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Modify: `crates/studio-import/src/source.rs`
- Modify: `crates/studio-import/src/lib.rs`
- Modify: `crates/studio-import/src/gltf.rs`
- Modify: `crates/studio-import/src/fbx.rs`
- Modify: `crates/studio-import/src/usd.rs`
- Modify: `crates/studio-import/src/ifc/geometry.rs`
- Modify: `crates/studio-import/src/step/mapping.rs`
- Modify: `crates/studio-import/src/obj/parser.rs`
- Modify: `crates/studio-import/src/obj/mtl.rs`
- Test: `crates/studio-import/tests/source_surface.rs`

**Interfaces:**
- Consumes: 无。
- Produces: `studio_assets::surface::{AlphaMode, TextureTransform, TextureSampler}`；`SourceMaterial::{alpha_mode, alpha_cutoff, double_sided, surface}`；`SourceScene.resources: Vec<SourceResource>`；`SourcePrimitive.uvs: Option<Vec<[f32; 2]>>`。

- [ ] **Step 1: Write the failing compatibility test**

```rust
use serde_json::json;
use studio_assets::surface::AlphaMode;
use studio_import::SourceMaterial;

#[test]
fn source_material_defaults_keep_old_records_compatible() {
    let material: SourceMaterial = serde_json::from_value(json!({
        "id": "steel",
        "name": "Steel",
        "base_color": [0.6, 0.6, 0.62],
        "opacity": 1.0,
        "extras": {}
    }))
    .unwrap();

    assert_eq!(material.alpha_mode, AlphaMode::Opaque);
    assert_eq!(material.alpha_cutoff, 0.5);
    assert!(!material.double_sided);
    assert!(material.surface.is_none());
}
```

- [ ] **Step 2: Run the test and verify the missing contract**

Run: `cargo test -p studio-import --test source_surface source_material_defaults_keep_old_records_compatible`

Expected: FAIL because new fields and `studio_assets::surface` do not exist.

- [ ] **Step 3: Add the shared scalar types**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureTransform {
    pub offset: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

impl Default for TextureTransform {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureSampler {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mag_filter: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_filter: Option<u32>,
    #[serde(default = "repeat_wrap")]
    pub wrap_s: u32,
    #[serde(default = "repeat_wrap")]
    pub wrap_t: u32,
}

impl Default for TextureSampler {
    fn default() -> Self {
        Self {
            mag_filter: None,
            min_filter: None,
            wrap_s: repeat_wrap(),
            wrap_t: repeat_wrap(),
        }
    }
}

const fn repeat_wrap() -> u32 {
    10_497
}
```

Export the module from `studio-assets/src/lib.rs`:

```rust
pub mod surface;
```

- [ ] **Step 4: Add the Source model types and serde defaults**

```rust
use studio_assets::surface::{AlphaMode, TextureSampler, TextureTransform};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub opacity: f32,
    #[serde(default)]
    pub alpha_mode: AlphaMode,
    #[serde(default = "default_alpha_cutoff")]
    pub alpha_cutoff: f32,
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<SourceSurface>,
    #[serde(with = "json_value")]
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSurface {
    pub metallic: f32,
    pub roughness: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<SourceTextureBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_texture: Option<SourceNormalTexture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occlusion_texture: Option<SourceTextureBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_roughness_texture: Option<SourceTextureBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceNormalTexture {
    pub binding: SourceTextureBinding,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceTextureBinding {
    pub resource: String,
    #[serde(default)]
    pub tex_coord: u32,
    #[serde(default)]
    pub transform: TextureTransform,
    #[serde(default)]
    pub sampler: TextureSampler,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceResource {
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

fn default_alpha_cutoff() -> f32 {
    0.5
}
```

Add `#[serde(default)] pub resources: Vec<SourceResource>` to `SourceScene`, and
`#[serde(default, skip_serializing_if = "Option::is_none")] pub uvs: Option<Vec<[f32; 2]>>`
to `SourcePrimitive`.

- [ ] **Step 5: Update every Source literal with historical defaults**

In every listed importer and test constructor, add:

```rust
alpha_mode: AlphaMode::Opaque,
alpha_cutoff: 0.5,
double_sided: false,
surface: None,
```

Add `uvs: None` to every `SourcePrimitive` literal and `resources: Vec::new()` to every
`SourceScene` literal. Update `to_canonical` only enough to compile; it deliberately still
ignores the new fields until Task 8.

- [ ] **Step 6: Run the compatibility test**

Run: `cargo test -p studio-import --test source_surface source_material_defaults_keep_old_records_compatible`

Expected: PASS.

- [ ] **Step 7: Run importer regressions**

Run: `cargo test -p studio-import`

Expected: PASS with existing glTF, OBJ, FBX, USD, STEP, IFC and Worker tests unchanged except mechanical struct defaults.

- [ ] **Step 8: Format and commit**

```bash
cargo fmt --all
git add crates/studio-assets/src/surface.rs crates/studio-assets/src/lib.rs crates/studio-import
git commit -m "feat(import): add material surface source contract"
```

### Task 2: Worker 协议 2 与图片资源载荷

**Files:**
- Modify: `crates/studio-import/src/worker.rs`
- Modify: `crates/studio-import/tests/worker_protocol.rs`
- Test: `crates/studio-import/src/worker.rs` unit tests

**Interfaces:**
- Consumes: Task 1 的 `SourceScene.resources`。
- Produces: `WORKER_PROTOCOL_VERSION = 2`；`WorkerResponse::V2` 成功响应；`WorkerResponse::V2Failure` 失败响应。

- [ ] **Step 1: Write the failing protocol test**

```rust
#[test]
fn protocol_two_round_trips_image_resources() {
    let mut scene = fixture_scene();
    scene.resources.push(studio_import::SourceResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3],
    });
    let response = WorkerResponse::V2 {
        source_scene: scene,
        warnings: Vec::new(),
    };
    let encoded =
        bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap();

    let decoded = decode_worker_response(&encoded).unwrap();

    assert_eq!(decoded.resources[0].bytes, [0x89, b'P', b'N', b'G', 0, 1, 2, 3]);
}
```

Add `SourceResource` to the test import list.

- [ ] **Step 2: Run the test and observe V2 rejection**

Run: `cargo test -p studio-import --test worker_protocol protocol_two_round_trips_image_resources`

Expected: FAIL with `unsupported response protocol 2`.

- [ ] **Step 3: Bump the protocol and accept only V2**

```rust
pub const WORKER_PROTOCOL_VERSION: u32 = 2;

pub enum WorkerResponse {
    V1 {
        source_scene: crate::SourceScene,
        warnings: Vec<ImportWarning>,
    },
    V1Failure {
        code: String,
        message: String,
    },
    V2 {
        source_scene: crate::SourceScene,
        warnings: Vec<ImportWarning>,
    },
    V2Failure {
        code: String,
        message: String,
    },
}
```

Make `execute_worker_request_inner` emit `WorkerResponse::V2`, make
`write_worker_failure` emit `V2Failure`, and change `decode_worker_response` to accept V2,
accept V2Failure after `validate_worker_error_code`, and return protocol errors for V1/V1Failure.

- [ ] **Step 4: Update protocol mismatch tests**

Change the existing wrong-version test to encode `WorkerResponse::V1` and expect
`unsupported response protocol 1`. Keep trailing-byte and structured-failure tests, switching
their success/failure variants to V2/V2Failure.

- [ ] **Step 5: Run Worker tests**

Run: `cargo test -p studio-import --test worker_protocol`

Expected: PASS for V2 success, V2 failure, wrong protocol, trailing bytes and resource bytes.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/studio-import/src/worker.rs crates/studio-import/tests/worker_protocol.rs
git commit -m "feat(import): carry image resources in worker protocol v2"
```

### Task 3: 有界图片资源读取与格式校验

**Files:**
- Create: `crates/studio-assets/src/image.rs`
- Modify: `crates/studio-assets/src/glb.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Modify: `crates/studio-import/src/limits.rs`
- Test: `crates/studio-assets/tests/image_resources.rs`
- Test: `crates/studio-import/tests/worker_protocol.rs`

**Interfaces:**
- Consumes: `SourceGlb` 的 `base_dir`、`buffers` 和现有 URI confinement。
- Produces: `ImageResource { digest: String, media_type: String, bytes: Vec<u8> }`; `read_image_uri(uri, base_dir, declared_media_type, max_bytes)`; `read_image_buffer_view(source, image, max_bytes)`; `validate_image_bytes(bytes, declared_media_type, max_bytes)`.

- [ ] **Step 1: Write failing format and confinement tests**

```rust
use std::path::Path;

use studio_assets::image::{read_image_uri, validate_image_bytes};

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

#[test]
fn identifies_supported_images_from_bytes() {
    let image = validate_image_bytes(PNG, Some("image/png"), 1024).unwrap();
    assert_eq!(image.media_type, "image/png");
    assert_eq!(image.digest.len(), 64);
}

#[test]
fn rejects_unknown_image_bytes() {
    let error = validate_image_bytes(b"<svg></svg>", None, 1024).unwrap_err();
    assert!(error.to_string().contains("unsupported image"));
}

#[test]
fn rejects_parent_directory_escape() {
    let root = tempfile::tempdir().unwrap();
    let error = read_image_uri("../outside.png", root.path(), None, 1024).unwrap_err();
    assert!(error.to_string().contains("escapes the source directory"));
}

#[cfg(unix)]
#[test]
fn rejects_a_symlink_that_escapes_the_source_directory() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("image.png"), PNG).unwrap();
    symlink(outside.path(), root.path().join("linked")).unwrap();

    let error = read_image_uri("linked/image.png", root.path(), None, 1024).unwrap_err();

    assert!(error.to_string().contains("escapes the source directory"));
}
```

- [ ] **Step 2: Run and verify missing image API**

Run: `cargo test -p studio-assets --test image_resources`

Expected: FAIL because `studio_assets::image` does not exist.

- [ ] **Step 3: Add bounded URI reading without changing existing callers**

In `glb.rs`, factor the current body of `read_bounded_uri` into:

```rust
pub fn read_bounded_uri_with_limit(
    uri: &str,
    base_dir: &Path,
    max_bytes: usize,
) -> AssetResult<Vec<u8>> {
    if let Some(payload) = uri.strip_prefix("data:") {
        let (metadata, data) = payload
            .split_once(',')
            .ok_or_else(|| invalid("data URI is invalid"))?;
        let maximum_encoded = if max_bytes == usize::MAX {
            None
        } else {
            Some(
                max_bytes
                    .checked_mul(4)
                    .and_then(|value| value.checked_div(3))
                    .and_then(|value| value.checked_add(8))
                    .ok_or_else(|| invalid("data URI size overflowed"))?,
            )
        };
        if maximum_encoded.is_some_and(|maximum| data.len() > maximum) {
            return Err(invalid("external resource exceeds its size limit"));
        }
        let bytes = if metadata.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| invalid("data URI payload is invalid"))?
        } else {
            percent_decode_str(data)
                .decode_utf8()
                .map_err(|_| invalid("data URI payload is invalid"))?
                .into_owned()
                .into_bytes()
        };
        if bytes.len() > max_bytes {
            return Err(invalid("external resource exceeds its size limit"));
        }
        return Ok(bytes);
    }

    if Path::new(uri).is_absolute() || looks_like_scheme(uri) {
        return Err(invalid("external resource URI must be relative"));
    }
    let decoded = percent_decode_str(uri)
        .decode_utf8()
        .map_err(|_| invalid("URI is not UTF-8"))?;
    let relative = Path::new(decoded.as_ref());
    if relative
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid(
            "external resource URI escapes the source directory",
        ));
    }
    if relative
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(invalid("external resource URI must be relative"));
    }
    let base =
        std::fs::canonicalize(base_dir).map_err(|_| invalid("source directory is missing"))?;
    let target = base.join(relative);
    let metadata =
        std::fs::symlink_metadata(&target).map_err(|_| invalid("external resource is missing"))?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("external resource must not be a symbolic link"));
    }
    let canonical = std::fs::canonicalize(&target)
        .map_err(|_| invalid("external resource is missing"))?;
    if !canonical.starts_with(&base) {
        return Err(invalid(
            "external resource URI escapes the source directory",
        ));
    }
    let size = std::fs::metadata(&canonical)
        .map_err(|_| invalid("external resource is unreadable"))?
        .len();
    if size > max_bytes as u64 {
        return Err(invalid("external resource exceeds its size limit"));
    }
    std::fs::read(canonical).map_err(|_| invalid("external resource is unreadable"))
}
```

Keep `pub fn read_bounded_uri(...)` delegating with `usize::MAX` so existing buffer and OBJ
behavior does not change.

- [ ] **Step 4: Add image format validation and content addressing**

```rust
use std::path::Path;

use sha2::{Digest, Sha256};
use studio_assets::{AssetError, AssetResult, SourceGlb, read_bounded_uri_with_limit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResource {
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

pub fn validate_image_bytes(
    bytes: Vec<u8>,
    declared_media_type: Option<&str>,
    max_bytes: usize,
) -> AssetResult<ImageResource> {
    if bytes.len() > max_bytes {
        return Err(AssetError::InvalidGlb(
            "external resource exceeds its size limit".into(),
        ));
    }
    let media_type = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err(AssetError::InvalidGlb("unsupported image format".into()));
    };
    if declared_media_type.is_some_and(|declared| declared != media_type) {
        return Err(AssetError::InvalidGlb(
            "image media type does not match its bytes".into(),
        ));
    }
    Ok(ImageResource {
        digest: format!("{:x}", Sha256::digest(&bytes)),
        media_type: media_type.into(),
        bytes,
    })
}

pub fn read_image_uri(
    uri: &str,
    base_dir: &Path,
    declared_media_type: Option<&str>,
    max_bytes: usize,
) -> AssetResult<ImageResource> {
    let bytes = read_bounded_uri_with_limit(uri, base_dir, max_bytes)?;
    validate_image_bytes(bytes, declared_media_type, max_bytes)
}
```

Implement `read_image_buffer_view` by validating `image.bufferView`, reading the referenced
bufferView bytes from `SourceGlb.buffers`, and passing the bytes and declared `mimeType` to
`validate_image_bytes`.

Define the three resource limits once in `studio-assets/src/image.rs`:

```rust
pub const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TEXTURE_TOTAL_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_TEXTURE_COUNT: usize = 4096;
```

Export the constants and use `MAX_TEXTURE_BYTES` as the default per-image reader limit.

- [ ] **Step 5: Export the image module**

```rust
pub mod image;
pub use glb::read_bounded_uri_with_limit;
pub use image::{
    ImageResource, MAX_TEXTURE_BYTES, MAX_TEXTURE_COUNT, MAX_TEXTURE_TOTAL_BYTES,
    read_image_buffer_view, read_image_uri, validate_image_bytes,
};
```

- [ ] **Step 6: Run the focused tests**

Run: `cargo test -p studio-assets --test image_resources`

Expected: PASS for PNG, unknown bytes, parent escape and Unix symlink escape. Add and run
separate assertions for over-limit data URI and external file before committing.

- [ ] **Step 7: Count resource bytes in scene limits**

Add this failing test to `worker_protocol.rs`:

```rust
#[test]
fn rejects_image_resources_above_the_scene_memory_limit() {
    let mut scene = fixture_scene();
    scene.resources.push(studio_import::SourceResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![0; 128],
    });
    let limits = NativeImportLimits {
        max_memory_bytes: 64,
        ..NativeImportLimits::default()
    };

    let error = validate_scene_limits(&scene, &limits).unwrap_err();

    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}
```

In `estimated_scene_bytes`, add `scene.resources.capacity() * size_of::<SourceResource>()`,
then each resource digest/media string capacity and byte capacity. Also reject more than
`MAX_TEXTURE_COUNT`, a resource above `MAX_TEXTURE_BYTES`, and a checked sum above
`MAX_TEXTURE_TOTAL_BYTES`; return `ImportError::LimitExceeded` before checking the broader
scene memory budget.

Run: `cargo test -p studio-import --test worker_protocol rejects_image_resources_above_the_scene_memory_limit`

Expected: PASS after the accounting change.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-assets/src crates/studio-assets/tests/image_resources.rs crates/studio-import/src/limits.rs crates/studio-import/tests/worker_protocol.rs
git commit -m "feat(assets): validate bounded image resources"
```

### Task 4: glTF 图片资源与 TEXCOORD_0

**Files:**
- Modify: `crates/studio-assets/src/accessor.rs`
- Modify: `crates/studio-assets/tests/source_reader.rs`
- Modify: `crates/studio-import/src/gltf.rs`
- Modify: `crates/studio-import/tests/gltf_import.rs`

**Interfaces:**
- Consumes: Task 1 的 `SourceScene.resources`、`SourcePrimitive.uvs`；Task 3 的 `ImageResource`。
- Produces: `read_accessor` 对 normalized UBYTE/USHORT 返回归一化 F32；glTF import 产出去重且按 digest 排序的 `SourceResource`；`TEXCOORD_0` 进入 `SourcePrimitive.uvs`。

- [ ] **Step 1: Write the failing normalized accessor test**

```rust
#[test]
fn normalized_unsigned_texcoords_decode_to_float_unit_range() {
    let (directory, path) = packed_fixture_normalized_uv();
    let source = studio_assets::read_glb(&path).unwrap();

    let values = studio_assets::read_accessor(&source, 1).unwrap();

    assert_eq!(values.as_f32().unwrap(), [0.0, 0.0, 65_535.0 / 65_535.0, 32_768.0 / 65_535.0]);
    drop(directory);
}
```

The helper writes a GLB with accessor 1 as `VEC2 / UNSIGNED_SHORT / normalized=true`.

- [ ] **Step 2: Run and observe normalized rejection**

Run: `cargo test -p studio-assets --test source_reader normalized_unsigned_texcoords_decode_to_float_unit_range`

Expected: FAIL with `normalized accessors are unsupported`.

- [ ] **Step 3: Decode normalized integer accessors as F32**

In `read_accessor`, read `normalized` instead of rejecting it. Add `COMPONENT_U8`, accept
U8/U16/U32/F32, and when normalized is true:

```rust
let divisor = match component_type {
    COMPONENT_U8 => u8::MAX as f32,
    COMPONENT_U16 => u16::MAX as f32,
    _ => {
        return Err(AssetError::AccessorUnsupported(
            "only unsigned integer accessors can be normalized".into(),
        ));
    }
};
```

Build the normal F32 value vector and return `AccessorData::F32`; preserve existing unnormalized
variants and their current index behavior.

- [ ] **Step 4: Write the failing glTF image/UV import test**

```rust
#[test]
fn imports_external_data_and_triangle_uvs_as_resources() {
    let root = tempfile::tempdir().unwrap();
    let texture_dir = root.path().join("textures");
    std::fs::create_dir(&texture_dir).unwrap();
    std::fs::write(texture_dir.join("base.png"), PNG_BYTES).unwrap();
    let path = root.path().join("uv.gltf");
    std::fs::write(&path, uv_gltf_json("textures/base.png")).unwrap();

    let scene = GltfImporter::new().import_file(&path, "uv").unwrap();

    assert_eq!(scene.meshes[0].primitives[0].uvs, Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]));
    assert_eq!(scene.resources.len(), 1);
    assert_eq!(scene.resources[0].media_type, "image/png");
    assert_eq!(scene.resources[0].digest.len(), 64);
}
```

`uv_gltf_json` writes F32 VEC3 positions, F32 VEC2 UVs, U16 indices, one image and
`"samplers": [{"wrapS": 10497, "wrapT": 10497}]`.

- [ ] **Step 5: Implement glTF UV and image loading**

Add:

```rust
fn optional_triangle_uvs(
    source: &SourceGlb,
    attributes: &Map<String, Value>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<Option<Vec<[f32; 2]>>, ImportError> {
    let Some(value) = attributes.get("TEXCOORD_0") else {
        return Ok(None);
    };
    let index = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ImportError::InvalidSource("TEXCOORD_0 accessor is invalid".into()))?;
    let accessor = source
        .json
        .get("accessors")
        .and_then(Value::as_array)
        .and_then(|accessors| accessors.get(index))
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("TEXCOORD_0 accessor is missing".into()))?;
    if accessor.contains_key("sparse")
        || accessor.get("type").and_then(Value::as_str) != Some("VEC2")
    {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED",
            "only dense VEC2 TEXCOORD_0 accessors are imported",
        ));
        return Ok(None);
    }
    let AccessorData::F32 { values, components: 2 } =
        read_accessor(source, index)
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    else {
        return Err(ImportError::InvalidSource(
            "TEXCOORD_0 must be a VEC2 accessor".into(),
        ));
    };
    Ok(Some(
        values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|value| [value[0], value[1]])
            .collect(),
    ))
}
```

Call it while importing primitives and set `uvs`. If the accessor uses a legal but unhandled
component encoding, return the same warning and `None`; malformed indices remain fatal.

Add a resource loader that:

1. rejects image index out of range;
2. handles `image.uri` with `read_image_uri`;
3. handles `image.bufferView` with `read_image_buffer_view`;
4. inserts into `BTreeMap<String, SourceResource>` keyed by digest;
5. errors if an identical digest has a different media type;
6. rejects more than `MAX_TEXTURE_COUNT`;
7. checks `MAX_TEXTURE_BYTES` before reading and a checked `MAX_TEXTURE_TOTAL_BYTES` sum before
   inserting a new resource.

Pass `image.mimeType` into `read_image_uri`/`validate_image_bytes` so declaration mismatches fail.

After materials are imported, move the map values into `SourceScene.resources`.
Move `warnings` initialization before material import so Task 5 can record texture warnings.
Remove `textures`, `images`, and `samplers` from `add_unsupported_resource_warnings`; animations,
skins and cameras remain unsupported.

- [ ] **Step 6: Run glTF and accessor tests**

Run: `cargo test -p studio-assets --test source_reader normalized_unsigned_texcoords_decode_to_float_unit_range`

Run: `cargo test -p studio-import --test gltf_import imports_external_data_and_triangle_uvs_as_resources`

Expected: PASS.

- [ ] **Step 7: Add embedded image coverage**

Add a GLB fixture in test code where `image.bufferView` points at a PNG and `mimeType` is
`image/png`. Assert one resource, no `ICE3_STUDIO_IMPORT_RESOURCE_UNSUPPORTED` warning for
`images`, and exact PNG bytes.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-assets/src/accessor.rs crates/studio-assets/tests/source_reader.rs crates/studio-import/src/gltf.rs crates/studio-import/tests/gltf_import.rs
git commit -m "feat(import): load gltf images and source uvs"
```

### Task 5: glTF 材质槽、transform、sampler 与降级

**Files:**
- Modify: `crates/studio-import/src/gltf.rs`
- Modify: `crates/studio-import/tests/gltf_import.rs`

**Interfaces:**
- Consumes: Task 1 的 Source surface/binding；Task 4 的图片资源表与 UV。
- Produces: `import_material` 解析四个贴图槽、PBR 因子、alpha、double-sided 和 `KHR_texture_transform`；稳定 warning codes。

- [ ] **Step 1: Write the failing complete-surface test**

```rust
#[test]
fn imports_all_surface_slots_and_texture_parameters() {
    let root = textured_surface_fixture();
    let scene = GltfImporter::new()
        .import_file(&root.join("surface.gltf"), "surface")
        .unwrap();
    let material = &scene.materials[0];
    let surface = material.surface.as_ref().unwrap();

    assert_eq!(surface.metallic, 0.8);
    assert_eq!(surface.roughness, 0.25);
    assert_eq!(surface.emissive, Some([0.0, 0.1, 0.2]));
    assert_eq!(surface.normal_texture.as_ref().unwrap().scale, 1.7);
    assert_eq!(surface.base_color_texture.as_ref().unwrap().tex_coord, 0);
    assert_eq!(surface.base_color_texture.as_ref().unwrap().transform.offset, [0.25, 0.5]);
    assert_eq!(surface.base_color_texture.as_ref().unwrap().sampler.wrap_s, 33071);
    assert_eq!(material.alpha_mode, AlphaMode::Mask);
    assert_eq!(material.alpha_cutoff, 0.3);
    assert!(material.double_sided);
}
```

The fixture uses four images with valid headers in `textures/` and one material carrying
baseColor, normal, occlusion and metallic-roughness textureInfo objects.

- [ ] **Step 2: Run and observe missing surface data**

Run: `cargo test -p studio-import --test gltf_import imports_all_surface_slots_and_texture_parameters`

Expected: FAIL because `surface` is `None`.

- [ ] **Step 3: Parse material state and factors**

Implement
`import_material(value, index, asset_id, source, resources, warnings)` and:

```rust
let pbr = material
    .get("pbrMetallicRoughness")
    .and_then(Value::as_object);
let alpha_mode = match material.get("alphaMode").and_then(Value::as_str).unwrap_or("OPAQUE") {
    "OPAQUE" => AlphaMode::Opaque,
    "MASK" => AlphaMode::Mask,
    "BLEND" => AlphaMode::Blend,
    other => {
        return Err(ImportError::InvalidSource(format!(
            "unsupported alphaMode {other}"
        )));
    }
};
let surface = Some(SourceSurface {
    metallic: factor(pbr.and_then(|value| value.get("metallicFactor")), 1.0)?,
    roughness: factor(pbr.and_then(|value| value.get("roughnessFactor")), 1.0)?,
    emissive: optional_vec3(material.get("emissiveFactor"), "emissiveFactor")?,
    base_color_texture: import_texture_info(
        source,
        resources,
        warnings,
        pbr.and_then(|value| value.get("baseColorTexture")),
        "baseColorTexture",
    )?,
    normal_texture: import_normal_texture(
        source,
        resources,
        warnings,
        material.get("normalTexture"),
    )?,
    occlusion_texture: import_texture_info(
        source,
        resources,
        warnings,
        material.get("occlusionTexture"),
        "occlusionTexture",
    )?,
    metallic_roughness_texture: import_texture_info(
        source,
        resources,
        warnings,
        pbr.and_then(|value| value.get("metallicRoughnessTexture")),
        "metallicRoughnessTexture",
    )?,
});
```

`factor` must reject non-finite values and clamp only values accepted by the current source
contract; alpha cutoff is range-checked to `[0,1]`.

- [ ] **Step 4: Implement textureInfo resolution**

```rust
fn import_texture_info(
    source: &SourceGlb,
    resources: &mut BTreeMap<String, SourceResource>,
    warnings: &mut BTreeSet<ImportWarning>,
    value: Option<&Value>,
    field: &str,
) -> Result<Option<SourceTextureBinding>, ImportError> {
    let Some(info) = value else {
        return Ok(None);
    };
    let info = info
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} is invalid")))?;
    let texture_index = index_field(info.get("index"), &format!("{field}.index"))?;
    let textures = source
        .json
        .get("textures")
        .and_then(Value::as_array)
        .ok_or_else(|| ImportError::InvalidSource("textures are missing".into()))?;
    let texture = textures
        .get(texture_index)
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("texture index is out of range".into()))?;
    let sampler = import_sampler(source, texture.get("sampler"))?;
    let mut tex_coord = info.get("texCoord").and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut transform = TextureTransform::default();
    if let Some(extension) = info
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|value| value.get("KHR_texture_transform"))
        .and_then(Value::as_object)
    {
        if let Some(overridden) = extension.get("texCoord").and_then(Value::as_u64) {
            tex_coord = u32::try_from(overridden)
                .map_err(|_| ImportError::InvalidSource("texCoord is too large".into()))?;
        }
        transform.offset = vector2(extension.get("offset"), [0.0, 0.0], "offset")?;
        transform.rotation = optional_finite(extension.get("rotation"), 0.0, "rotation")?;
        transform.scale = vector2(extension.get("scale"), [1.0, 1.0], "scale")?;
    }
    if tex_coord != 0 {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED",
            &format!("{field} uses TEXCOORD_{tex_coord}; only TEXCOORD_0 is imported"),
        ));
        return Ok(None);
    }
    let image_index = index_field(texture.get("source"), "texture.source")?;
    let resource = load_image(source, resources, image_index)?;
    Ok(Some(SourceTextureBinding {
        resource: resource.digest.clone(),
        tex_coord,
        transform,
        sampler,
    }))
}
```

`normalTexture` uses the same resolution and reads `scale` with default `1.0`.
Add `vector2(value, default, field)` beside the existing `vector3`, and
`optional_finite(value, default, field)` beside the existing `finite`; both reject non-finite
numbers with `ICE3_STUDIO_IMPORT_INVALID_SOURCE`.

`import_sampler` returns `TextureSampler::default()` when `texture.sampler` is absent. When it is
present, resolve the sampler index, then copy `magFilter`, `minFilter`, `wrapS` and `wrapT`;
accept only values in the glTF sampler enums and reject non-integer or out-of-range values.

- [ ] **Step 5: Add warning behavior tests**

```rust
#[test]
fn skips_a_texture_bound_to_an_unsupported_uv_channel() {
    let root = texture_fixture_with_texcoord(1);
    let scene = GltfImporter::new()
        .import_file(&root.join("surface.gltf"), "surface")
        .unwrap();

    assert!(scene.materials[0]
        .surface
        .as_ref()
        .unwrap()
        .base_color_texture
        .is_none());
    assert!(scene.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED"
    }));
}
```

```rust
#[test]
fn warns_for_emissive_textures_and_unknown_material_extensions() {
    let root = texture_fixture_with_material_extension();
    let scene = GltfImporter::new()
        .import_file(&root.join("surface.gltf"), "surface")
        .unwrap();

    assert!(scene.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED"
    }));
    assert!(scene.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED"
            && warning.message.contains("KHR_materials_unlit")
    }));
}
```

- [ ] **Step 6: Implement warning gates before image loading**

Resolve texture, UV channel and transform first. If effective `tex_coord != 0`, return `None`
and insert the coordinate warning before loading image bytes. That keeps an unsupported channel
from requiring a file that will not be used.

If `material.emissiveTexture` exists, insert
`ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED` and import only `emissiveFactor`. For each key
under `material.extensions` that is not handled by the base glTF material contract, insert
`ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED` with the extension name in a deterministic
message; do not copy unknown extension JSON into surface fields.

- [ ] **Step 7: Run full glTF import tests**

Run: `cargo test -p studio-import --test gltf_import`

Expected: PASS for complete surface, embedded/external/data images, transform, sampler,
alpha, double-sided and warning gates.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-import/src/gltf.rs crates/studio-import/tests/gltf_import.rs
git commit -m "feat(import): preserve gltf material surfaces"
```

### Task 6: Canonical Surface 类型与 legacy 兼容

**Files:**
- Modify: `crates/studio-assets/src/canonical.rs`
- Modify: `crates/studio-assets/tests/canonical_writer.rs`
- Modify: `crates/studio-assets/tests/prepare_artifact.rs`
- Modify: `crates/studio-assets/tests/texture_pipeline.rs`
- Modify: `crates/studio-assets/tests/common/mod.rs`
- Modify: `crates/studio-pipeline/tests/lod.rs`
- Test: `crates/studio-assets/tests/canonical_writer.rs`

**Interfaces:**
- Consumes: Task 1 的共享 `AlphaMode`、`TextureTransform`、`TextureSampler`。
- Produces: `CanonicalSurface`、`CanonicalTextureBinding`、`CanonicalNormalTexture`、`CanonicalResource`；`CanonicalMaterial::{alpha_mode, alpha_cutoff, double_sided, surface}`；`CanonicalScene.resources`；`CanonicalPrimitive.uvs`。

- [ ] **Step 1: Write the failing legacy-default test**

```rust
#[test]
fn canonical_material_defaults_keep_legacy_texture_behavior() {
    let scene = fixture_scene();
    let bytes = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(&bytes)),
        "e3da988f73c5955b395b89d28268ff93a0004ddfd17def9a3765a66b24768d6d"
    );
    let parsed = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();

    assert_eq!(
        parsed.json["materials"][0]["alphaMode"],
        "BLEND"
    );
    assert_eq!(
        parsed.json["materials"][0]["pbrMetallicRoughness"]["metallicFactor"],
        0.0
    );
    assert!(parsed.json["materials"][0].get("doubleSided").is_none());
}
```

The test must use the existing `CanonicalMaterial` literal updated with:

```rust
alpha_mode: AlphaMode::Opaque,
alpha_cutoff: 0.5,
double_sided: false,
surface: None,
```

`surface: None` preserves the historical rule `opacity < 1 => BLEND`; the new
`alpha_mode` is authoritative only when a surface is present. This distinction is required
because older canonical callers did not carry alpha mode.

- [ ] **Step 2: Run and observe missing fields/types**

Run: `cargo test -p studio-assets --test canonical_writer canonical_material_defaults_keep_legacy_texture_behavior`

Expected: FAIL because the new canonical fields and `AlphaMode` import do not exist.

- [ ] **Step 3: Add canonical surface types**

```rust
use crate::surface::{AlphaMode, TextureSampler, TextureTransform};

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalSurface {
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: Option<[f32; 3]>,
    pub base_color_texture: Option<CanonicalTextureBinding>,
    pub normal_texture: Option<CanonicalNormalTexture>,
    pub occlusion_texture: Option<CanonicalTextureBinding>,
    pub metallic_roughness_texture: Option<CanonicalTextureBinding>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalNormalTexture {
    pub binding: CanonicalTextureBinding,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalTextureBinding {
    pub resource: String,
    pub tex_coord: u32,
    pub transform: TextureTransform,
    pub sampler: TextureSampler,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResource {
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}
```

Add to `CanonicalMaterial`:

```rust
pub alpha_mode: AlphaMode,
pub alpha_cutoff: f32,
pub double_sided: bool,
pub surface: Option<CanonicalSurface>,
```

Add `pub resources: Vec<CanonicalResource>` to `CanonicalScene` and
`pub uvs: Option<Vec<[f32; 2]>>` to `CanonicalPrimitive`.

- [ ] **Step 4: Update every canonical literal mechanically**

Add the historical defaults to all material literals:

```rust
alpha_mode: AlphaMode::Opaque,
alpha_cutoff: 0.5,
double_sided: false,
surface: None,
```

Add `uvs: None` to all primitive literals and `resources: Vec::new()` to all scene literals.
Do not change writer behavior in this task.

- [ ] **Step 5: Add validation tests for invalid material state**

```rust
#[test]
fn rejects_invalid_alpha_and_surface_values() {
    let mut scene = fixture_scene();
    scene.materials[0].alpha_cutoff = f32::NAN;
    let error = write_prepared_glb(&scene, &options("invalid")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");

    let mut scene = fixture_scene();
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 1.0,
        roughness: 1.0,
        emissive: None,
        base_color_texture: None,
        normal_texture: None,
        occlusion_texture: None,
        metallic_roughness_texture: None,
    });
    scene.materials[0].surface.as_mut().unwrap().metallic = -0.1;
    let error = write_prepared_glb(&scene, &options("invalid")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}
```

- [ ] **Step 6: Extend `validate_materials` minimally**

```rust
if !material.alpha_cutoff.is_finite()
    || !(0.0..=1.0).contains(&material.alpha_cutoff)
    || material.surface.as_ref().is_some_and(|surface| {
        !(0.0..=1.0).contains(&surface.metallic)
            || !(0.0..=1.0).contains(&surface.roughness)
            || surface.emissive.is_some_and(|value| {
                value.iter().any(|component| !component.is_finite() || *component < 0.0)
            })
    })
{
    return Err(invalid("material is invalid"));
}
```

Leave resource/UV length validation for Task 7.

- [ ] **Step 7: Run canonical tests**

Run: `cargo test -p studio-assets`

Expected: PASS for legacy defaults and invalid alpha/surface checks; existing output remains unchanged.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-assets/src/canonical.rs crates/studio-assets/tests crates/studio-pipeline/tests/lod.rs
git commit -m "feat(assets): add canonical surface contract"
```

### Task 7: Canonical writer 输出四类贴图与原始 UV

**Files:**
- Modify: `crates/studio-assets/src/canonical.rs`
- Modify: `crates/studio-assets/tests/texture_pipeline.rs`
- Modify: `crates/studio-assets/tests/canonical_writer.rs`

**Interfaces:**
- Consumes: Task 6 的 canonical surface/resource/UV 类型；现有 `BinaryBuilder` 和 material index deduplication。
- Produces: `texture_registry(...)` 输出稳定 image/sampler/texture 表；`write_texture_info(...)`；primitive `TEXCOORD_0` accessor；legacy `texture` 与 surface base-color 冲突检测。

- [ ] **Step 1: Write the failing four-slot/UV test**

```rust
#[test]
fn writes_four_surface_slots_uvs_and_transform() {
    let mut scene = scene_with(None);
    let base = resource(b"base");
    let normal = resource(b"normal");
    let occlusion = resource(b"occlusion");
    let orm = resource(b"orm");
    scene.resources = vec![base.clone(), normal.clone(), occlusion.clone(), orm.clone()];
    scene.meshes[0].primitives[0].uvs =
        Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.8,
        roughness: 0.25,
        emissive: Some([0.0, 0.1, 0.2]),
        base_color_texture: Some(binding(&base, 0, [2.0, 3.0])),
        normal_texture: Some(CanonicalNormalTexture {
            binding: binding(&normal, 0, [1.0, 1.0]),
            scale: 1.7,
        }),
        occlusion_texture: Some(binding(&occlusion, 0, [1.0, 1.0])),
        metallic_roughness_texture: Some(binding(&orm, 0, [0.5, 0.25])),
    });

    let artifact = prepare_canonical(&scene, options()).unwrap();
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();
    let material = &parsed.json["materials"][0];

    assert_eq!(material["pbrMetallicRoughness"]["metallicFactor"], 0.8);
    assert_eq!(material["pbrMetallicRoughness"]["roughnessFactor"], 0.25);
    assert_eq!(material["normalTexture"]["scale"], 1.7);
    assert!(material["pbrMetallicRoughness"]["baseColorTexture"]["index"].is_number());
    assert!(material["pbrMetallicRoughness"]["metallicRoughnessTexture"]["index"].is_number());
    assert!(material["normalTexture"]["index"].is_number());
    assert!(material["occlusionTexture"]["index"].is_number());
    let uv_index = parsed.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(parsed.json["accessors"][uv_index]["count"], 3);
    assert_eq!(parsed.json["accessors"][uv_index]["type"], "VEC2");
}
```

Define helpers in the test:

```rust
fn resource(bytes: &[u8]) -> CanonicalResource {
    CanonicalResource {
        digest: format!("{:x}", sha2::Sha256::digest(bytes)),
        media_type: "image/png".into(),
        bytes: bytes.to_vec(),
    }
}

fn binding(
    resource: &CanonicalResource,
    tex_coord: u32,
    scale: [f32; 2],
) -> CanonicalTextureBinding {
    CanonicalTextureBinding {
        resource: resource.digest.clone(),
        tex_coord,
        transform: TextureTransform {
            scale,
            ..TextureTransform::default()
        },
        sampler: TextureSampler::default(),
    }
}
```

- [ ] **Step 2: Run and observe surface fields ignored**

Run: `cargo test -p studio-assets --test texture_pipeline writes_four_surface_slots_uvs_and_transform`

Expected: FAIL because metallic remains `0.0` and normal/occlusion slots are absent.

- [ ] **Step 3: Build stable image, sampler and texture registries**

```rust
#[derive(Default)]
struct TextureRegistry {
    images: BTreeMap<ImageKey, usize>,
    samplers: BTreeMap<SamplerKey, usize>,
    textures: BTreeMap<(usize, usize), usize>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ImageKey {
    Resource(String),
    External(String),
}

impl TextureRegistry {
    fn texture(&mut self, resource: &CanonicalResource, sampler: TextureSampler) -> AssetResult<usize> {
        let image_key = ImageKey::Resource(resource.digest.clone());
        let image = match self.images.get(&image_key) {
            Some(index) => *index,
            None => {
                let index = self.images.len();
                self.images.insert(image_key, index);
                index
            }
        };
        let sampler_index = match self.samplers.get(&sampler.into()) {
            Some(index) => *index,
            None => {
                let index = self.samplers.len();
                self.samplers.insert(sampler.into(), index);
                index
            }
        };
        let key = (image, sampler_index);
        Ok(match self.textures.get(&key) {
            Some(index) => *index,
            None => {
                let index = self.textures.len();
                self.textures.insert(key, index);
                index
            }
        })
    }
}
```

`SamplerKey` is a derived `Ord` copy of `TextureSampler`. Validate that every binding resource
digest exists exactly once in `scene.resources` and bytes rehash to the declared digest. Add a
`texture_external(uri, sampler)` method that uses `ImageKey::External(uri.to_owned())`, so legacy
`CanonicalMaterial.texture` continues to reference the caller-provided path without inventing
resource bytes. When emitting `images`, sort `ImageKey` and emit either the prepared namespace
URI for `Resource` or the existing relative URI for `External`.

- [ ] **Step 4: Write textureInfo helper and material parameters**

```rust
fn write_texture_info(
    binding: &CanonicalTextureBinding,
    resources: &BTreeMap<&str, &CanonicalResource>,
    registry: &mut TextureRegistry,
) -> AssetResult<Value> {
    let resource = resources
        .get(binding.resource.as_str())
        .ok_or_else(|| invalid("texture resource is missing"))?;
    if binding.tex_coord != 0 {
        return Err(invalid("only TEXCOORD_0 can be prepared"));
    }
    let index = registry.texture(resource, binding.sampler)?;
    let transform = &binding.transform;
    Ok(json!({
        "index": index,
        "texCoord": 0,
        "extensions": {
            "KHR_texture_transform": {
                "offset": transform.offset,
                "rotation": transform.rotation,
                "scale": transform.scale
            }
        }
    }))
}
```

In `output_materials`, emit:

- real metallic/roughness factors when surface exists;
- `emissiveFactor`;
- base-color texture or legacy `material.texture`;
- normal texture plus `scale`;
- occlusion and metallic-roughness textures;
- `doubleSided` only when true;
- alphaMode mapping for `surface: Some`: Opaque, Mask + cutoff, Blend + base alpha;
  for `surface: None`, retain the historical `opacity < 1 => BLEND` rule byte-for-byte.

Legacy `material.texture` remains a base-color path and uses its `uv_scale`. If both legacy and
surface base-color exist and their resource/path, UV scale, transform or sampler differ, return
`AssetError::InvalidGlb("legacy and surface base-color textures conflict")`.

- [ ] **Step 5: Emit `TEXCOORD_0` only when source UV or an actual texture requires it**

```rust
let material_uses_texture = source_material.is_some_and(|index| {
    context.source_materials[index]
        .surface
        .as_ref()
        .is_some_and(surface_has_texture)
        || context.source_materials[index].texture.is_some()
});
if primitive.uvs.is_some() || material_uses_texture {
    let uvs = if let Some(uvs) = &primitive.uvs {
        if uvs.len() != primitive.positions.len()
            || uvs.iter().flatten().any(|value| !value.is_finite())
        {
            return Err(invalid("primitive UVs are invalid"));
        }
        uvs.clone()
    } else {
        box_projected_uvs(&primitive.positions, primitive.normals.as_deref())
    };
    let values = uvs.iter().flat_map(|value| *value).collect::<Vec<_>>();
    let view = context
        .binary
        .append(bytemuck::cast_slice(&values), Some(34962));
    let accessor = context.binary.accessor(json!({
        "bufferView": view,
        "componentType": 5126,
        "count": uvs.len(),
        "type": "VEC2"
    }));
    attributes.insert("TEXCOORD_0".into(), Value::from(accessor));
}
```

Implement `box_projected_uvs` in `canonical.rs` from the existing Studio Core dominant-axis
algorithm so imported and non-imported canonical geometry use identical data without coupling
through `EvaluatedMesh`. `surface_has_texture` returns true only when at least one of the four
surface slots is `Some`.

- [ ] **Step 6: Write the sampler-dedup test**

```rust
#[test]
fn one_image_with_two_samplers_gets_two_texture_indices() {
    let mut scene = scene_with(None);
    let shared = resource(b"shared");
    scene.resources = vec![shared.clone()];
    let mut first = binding(&shared, 0, [1.0, 1.0]);
    let mut second = first.clone();
    second.sampler.wrap_s = 33071;
    scene.materials[0].surface = Some(CanonicalSurface {
        metallic: 0.0,
        roughness: 1.0,
        emissive: None,
        base_color_texture: Some(first),
        normal_texture: None,
        occlusion_texture: Some(second),
        metallic_roughness_texture: None,
    });

    let artifact = prepare_canonical(&scene, options()).unwrap();
    let parsed = read_glb_bytes(&artifact.bytes, &std::env::temp_dir()).unwrap();

    assert_eq!(parsed.json["images"].as_array().unwrap().len(), 1);
    assert_eq!(parsed.json["samplers"].as_array().unwrap().len(), 2);
    assert_eq!(parsed.json["textures"].as_array().unwrap().len(), 2);
    assert_ne!(
        parsed.json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"],
        parsed.json["materials"][0]["occlusionTexture"]["index"]
    );
}
```

- [ ] **Step 7: Run the focused tests**

Run: `cargo test -p studio-assets --test texture_pipeline`

Expected: PASS for all slots, transforms, UV accessor, legacy texture, no-UV fallback and
sampler-aware texture deduplication.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-assets/src/canonical.rs crates/studio-assets/tests/texture_pipeline.rs crates/studio-assets/tests/canonical_writer.rs
git commit -m "feat(assets): write complete material surfaces"
```

### Task 8: Source 到 Canonical 的表面与资源映射

**Files:**
- Modify: `crates/studio-import/src/lib.rs`
- Create: `crates/studio-import/tests/material_mapping.rs`

**Interfaces:**
- Consumes: Task 1 Source Surface/resources/UV；Task 6 Canonical Surface/resources/UV。
- Produces: `to_canonical` 逐字段复制 factor、四槽、transform、sampler、alpha、double-sided、UV 和去重资源。

- [ ] **Step 1: Write the failing mapping test**

```rust
use serde_json::json;
use studio_assets::surface::{AlphaMode, TextureSampler, TextureTransform};
use studio_import::{
    SourceMaterial, SourceMesh, SourceNormalTexture, SourcePrimitive, SourceResource, SourceScene,
    SourceSurface, SourceTextureBinding, to_canonical,
};

#[test]
fn maps_all_surface_fields_and_source_uvs() {
    let scene = SourceScene {
        asset_id: "mapped".into(),
        source_format: studio_import::SourceFormat::Gltf,
        nodes: Vec::new(),
        meshes: vec![SourceMesh {
            id: "mesh".into(),
            primitives: vec![SourcePrimitive {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                normals: None,
                colors: None,
                uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
                indices: vec![0, 1, 2],
                material: Some(0),
            }],
            extras: json!({}),
        }],
        materials: vec![SourceMaterial {
            id: "steel".into(),
            name: "Steel".into(),
            base_color: [0.2, 0.3, 0.4],
            opacity: 1.0,
            alpha_mode: AlphaMode::Blend,
            alpha_cutoff: 0.4,
            double_sided: true,
            surface: Some(SourceSurface {
                metallic: 0.8,
                roughness: 0.2,
                emissive: Some([0.0, 0.1, 0.2]),
                base_color_texture: Some(SourceTextureBinding {
                    resource: "a".repeat(64),
                    tex_coord: 0,
                    transform: TextureTransform::default(),
                    sampler: TextureSampler::default(),
                }),
                normal_texture: Some(SourceNormalTexture {
                    binding: SourceTextureBinding {
                        resource: "b".repeat(64),
                        tex_coord: 0,
                        transform: TextureTransform::default(),
                        sampler: TextureSampler::default(),
                    },
                    scale: 1.5,
                }),
                occlusion_texture: None,
                metallic_roughness_texture: None,
            }),
            extras: json!({}),
        }],
        resources: vec![SourceResource {
            digest: "a".repeat(64),
            media_type: "image/png".into(),
            bytes: vec![1, 2, 3],
        }],
        warnings: Vec::new(),
        asset_extras: json!({}),
    };

    let canonical = to_canonical(&scene);

    assert_eq!(canonical.materials[0].alpha_mode, AlphaMode::Blend);
    assert_eq!(canonical.materials[0].alpha_cutoff, 0.4);
    assert_eq!(canonical.materials[0].surface.as_ref().unwrap().metallic, 0.8);
    assert_eq!(canonical.materials[0].surface.as_ref().unwrap().normal_texture.as_ref().unwrap().scale, 1.5);
    assert_eq!(canonical.meshes[0].primitives[0].uvs.as_ref().unwrap().len(), 3);
    assert_eq!(canonical.resources, vec![studio_assets::canonical::CanonicalResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![1, 2, 3],
    }]);
}
```

- [ ] **Step 2: Run and observe ignored fields**

Run: `cargo test -p studio-import --test material_mapping maps_all_surface_fields_and_source_uvs`

Expected: FAIL because `to_canonical` still writes defaults and empty resources.

- [ ] **Step 3: Implement exact field mapping**

```rust
meshes: source.meshes.iter().map(|mesh| studio_assets::canonical::CanonicalMesh {
    id: mesh.id.clone(),
    primitives: mesh.primitives.iter().map(|primitive| {
        studio_assets::canonical::CanonicalPrimitive {
            positions: primitive.positions.clone(),
            normals: primitive.normals.clone(),
            colors: primitive.colors.clone(),
            uvs: primitive.uvs.clone(),
            indices: primitive.indices.clone(),
            material: primitive.material,
        }
    }).collect(),
    extras: mesh.extras.clone(),
}).collect(),
materials: source.materials.iter().map(|material| {
    studio_assets::canonical::CanonicalMaterial {
        id: material.id.clone(),
        name: material.name.clone(),
        base_color: material.base_color,
        opacity: material.opacity,
        alpha_mode: material.alpha_mode,
        alpha_cutoff: material.alpha_cutoff,
        double_sided: material.double_sided,
        surface: material.surface.as_ref().map(map_surface),
        texture: None,
        extras: material.extras.clone(),
    }
}).collect(),
resources: source.resources.iter().map(|resource| {
    studio_assets::canonical::CanonicalResource {
        digest: resource.digest.clone(),
        media_type: resource.media_type.clone(),
        bytes: resource.bytes.clone(),
    }
}).collect(),
```

Implement `map_surface` and `map_binding` as direct struct conversions. Do not clone-modify
JSON, and do not fabricate resource paths.

- [ ] **Step 4: Keep mapping pure and validate at prepare**

`to_canonical` remains infallible and performs direct structure mapping. Missing resource
digests, duplicate digests and digest/byte mismatches are rejected by Task 7 canonical
validation and Task 9 bundle preparation before any artifact is emitted.

```rust
fn map_binding(
    binding: &crate::SourceTextureBinding,
) -> studio_assets::canonical::CanonicalTextureBinding {
    studio_assets::canonical::CanonicalTextureBinding {
        resource: binding.resource.clone(),
        tex_coord: binding.tex_coord,
        transform: binding.transform,
        sampler: binding.sampler,
    }
}
```

Do not change the `to_canonical` signature in this task.

- [ ] **Step 5: Run mapping and importer suites**

Run: `cargo test -p studio-import`

Expected: PASS for mapping and all existing importers with default fields.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
git add crates/studio-import/src/lib.rs crates/studio-import/tests/material_mapping.rs
git commit -m "feat(import): map surfaces into canonical assets"
```

### Task 9: Prepared bundle、纹理命名空间与 profile 校验

**Files:**
- Modify: `crates/studio-assets/src/prepare.rs`
- Modify: `crates/studio-assets/src/canonical.rs`
- Modify: `crates/studio-assets/src/prepared.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Modify: `crates/studio-assets/tests/texture_pipeline.rs`
- Modify: `crates/studio-assets/tests/prepared_profile.rs`

**Interfaces:**
- Consumes: Task 7/8 的 canonical surface、resources、UV。
- Produces: `PrepareOptions.texture_namespace: String`; `PreparedResource`; `PreparedAssetBundle { glb, texture_uri_prefix, resources, metrics, preparation, budget, content_digest, warnings }`; `prepare_canonical_bundle(...)`。

- [ ] **Step 1: Write the failing bundle namespace test**

```rust
#[test]
fn bundle_names_resources_inside_the_requested_directory() {
    let mut scene = scene_with(None);
    scene.resources = vec![CanonicalResource {
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: PNG_BYTES.to_vec(),
    }];
    let mut options = options();
    options.texture_namespace = "plant.textures".into();
    scene.materials[0].surface = Some(surface_with_base("a".repeat(64)));

    let bundle = prepare_canonical_bundle(&scene, options).unwrap();
    let parsed = read_glb_bytes(&bundle.glb, &std::env::temp_dir()).unwrap();

    assert_eq!(
        parsed.json["images"][0]["uri"],
        format!("plant.textures/{}.png", "a".repeat(64))
    );
    assert_eq!(bundle.resources[0].uri, format!("plant.textures/{}.png", "a".repeat(64)));
    assert_eq!(bundle.resources[0].bytes, PNG_BYTES);
}
```

- [ ] **Step 2: Run and observe missing bundle API**

Run: `cargo test -p studio-assets --test texture_pipeline bundle_names_resources_inside_the_requested_directory`

Expected: FAIL because `prepare_canonical_bundle`, namespace and resource output do not exist.

- [ ] **Step 3: Add bundle types and move rather than duplicate GLB bytes**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedResource {
    pub uri: String,
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedAssetBundle {
    pub glb: Vec<u8>,
    pub texture_uri_prefix: String,
    pub resources: Vec<PreparedResource>,
    pub metrics: AssetMetrics,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
    pub content_digest: String,
    pub warnings: Vec<PrepareWarning>,
}
```

Implement:

```rust
pub fn prepare_canonical_bundle(
    scene: &CanonicalScene,
    options: PrepareOptions,
) -> Result<PreparedAssetBundle, AssetError> {
    let namespace = options.texture_namespace.clone();
    let texture_uri_prefix = texture_uri_prefix(&namespace)?;
    let artifact = prepare_canonical(scene, options)?;
    let resources = prepared_resources(scene, &texture_uri_prefix)?;
    Ok(PreparedAssetBundle {
        glb: artifact.bytes,
        texture_uri_prefix,
        resources,
        metrics: artifact.metrics,
        preparation: artifact.preparation,
        budget: artifact.budget,
        content_digest: artifact.content_digest,
        warnings: artifact.warnings,
    })
}
```

Add `texture_namespace: String` to `PrepareOptions`. All existing constructors use an empty
string unless they emit bundle resources.

- [ ] **Step 4: Generate URI from digest and media type**

```rust
fn prepared_resources(
    scene: &CanonicalScene,
    texture_uri_prefix: &str,
) -> Result<Vec<PreparedResource>, AssetError> {
    scene
        .resources
        .iter()
        .map(|resource| {
            let extension = match resource.media_type.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                other => {
                    return Err(invalid(&format!("unsupported image media type {other}")));
                }
            };
            let uri = format!("{texture_uri_prefix}/{}.{}", resource.digest, extension);
            Ok(PreparedResource {
                uri,
                digest: resource.digest.clone(),
                media_type: resource.media_type.clone(),
                bytes: resource.bytes.clone(),
            })
        })
        .collect()
}
```

Retain `.`, `-`, `_`, and ASCII alphanumeric; encode only characters not allowed in a URI path
segment. Reject empty digest, non-lowercase digest and digests whose bytes do not rehash.

Implement the encoder explicitly:

```rust
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b':')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

fn percent_encode_path_segment(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).to_string()
}

fn texture_uri_prefix(namespace: &str) -> Result<String, AssetError> {
    if namespace.contains('/') || namespace.contains('\\') {
        return Err(invalid("texture namespace must be one path segment"));
    }
    Ok(if namespace.is_empty() {
        "textures".into()
    } else {
        percent_encode_path_segment(namespace)
    })
}
```

Use one `resource_uri(digest, media_type, namespace)` helper in both
`prepared_resources(...)` and canonical image emission. `output_materials` must receive the
same image URI map; it cannot independently default to `textures/...` after namespace
preparation. Emit `mimeType` on Prepared GLB image objects.

- [ ] **Step 5: Write profile validation failures**

```rust
#[test]
fn rejects_out_of_range_extra_texture_slots() {
    let mut json = minimal_prepared_json();
    json["materials"][0]["normalTexture"] = json!({"index": 4});
    let source = read_glb_bytes(&pack_glb(
        &serde_json::to_vec(&json).unwrap(),
        &minimal_binary(),
    ), &std::env::temp_dir()).unwrap();

    let error = validate_prepared_profile(&source).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_INVALID");
}

#[test]
fn rejects_a_texture_uri_that_escapes_the_asset_directory() {
    let mut json = minimal_prepared_json();
    json["images"] = json!([{"uri": "../outside.png"}]);
    json["textures"] = json!([{"source": 0}]);
    json["materials"][0]["occlusionTexture"] = json!({"index": 0});

    let source = read_glb_bytes(&pack_glb(
        &serde_json::to_vec(&json).unwrap(),
        &minimal_binary(),
    ), &std::env::temp_dir()).unwrap();

    assert!(validate_prepared_profile(&source).is_err());
}
```

- [ ] **Step 6: Extend profile checks**

Validate:

- all four material textureInfo pointers;
- texCoord is 0 in v1 profile;
- normal scale is finite and `[0, 2]`;
- transform offset/rotation/scale are finite and scale components are positive;
- sampler indices are in range and sampler enum values are valid;
- texture source indices are valid;
- every image has a relative URI, no `..`, no absolute path, no scheme, no bufferView;
- image `mimeType`, when present, matches the URI extension.

- [ ] **Step 7: Run assets tests**

Run: `cargo test -p studio-assets`

Expected: PASS for bundle namespace, digest URIs, resource bytes, four-slot profile checks and
legacy texture output.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
git add crates/studio-assets/src crates/studio-assets/tests
git commit -m "feat(assets): prepare textured asset bundles"
```

### Task 10: CLI 事务发布 GLB、纹理目录与 report

**Files:**
- Create: `crates/studio-cli/src/publish.rs`
- Modify: `crates/studio-cli/src/lib.rs`
- Modify: `crates/studio-cli/src/import.rs`
- Modify: `crates/studio-cli/src/prepare_command.rs`
- Modify: `crates/studio-cli/tests/import.rs`
- Modify: `crates/studio-cli/tests/prepare_command.rs`

**Interfaces:**
- Consumes: Task 9 `PreparedAssetBundle` 和 `prepare_canonical_bundle`。
- Produces: `texture_namespace(output: &Path) -> Result<String, PublishError>`; `publish_asset_bundle(bundle, output, report, force) -> Result<(), PublishError>`; import/build 发布 `plant.glb` 与 `plant.textures/`。

- [ ] **Step 1: Write the failing transparent import test**

```rust
#[test]
fn import_publishes_glb_and_every_referenced_texture() {
    let root = textured_gltf_fixture();
    let output = root.path().join("plant.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "import",
            root.path().join("surface.gltf").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "plant",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let parsed = studio_assets::read_glb(&output).unwrap();
    let uri = parsed.json["images"][0]["uri"].as_str().unwrap();
    assert!(uri.starts_with("plant.textures/"));
    let texture = output.parent().unwrap().join(uri);
    assert!(texture.is_file());
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(std::fs::read(texture).unwrap())),
        format!("{:x}", sha2::Sha256::digest(PNG_BYTES))
    );
}
```

- [ ] **Step 2: Run and observe missing sidecar**

Run: `cargo test -p studio-cli --test import import_publishes_glb_and_every_referenced_texture`

Expected: FAIL because the GLB references `plant.textures/...` but the directory is not published.

- [ ] **Step 3: Add namespace derivation**

```rust
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use studio_assets::{PreparedAssetBundle, PreparedResource};

#[derive(Debug)]
pub enum PublishError {
    OutputExists,
    InvalidResource(String),
    Io(std::io::Error),
    RollbackFailed {
        primary: Box<PublishError>,
        details: Vec<String>,
    },
}

impl From<std::io::Error> for PublishError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn texture_namespace(output: &Path) -> Result<String, PublishError> {
    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PublishError::InvalidResource("output has no file stem".into()))?;
    Ok(format!("{stem}.textures"))
}
```

The physical directory is `output.with_file_name(texture_namespace(output))`; the URI version
uses percent-encoded path segments while preserving the same decoded directory name.
Reject a report path equal to the GLB output or texture directory before creating staging.

- [ ] **Step 4: Implement staged bundle publication**

```rust
pub fn publish_asset_bundle(
    bundle: &PreparedAssetBundle,
    output: &Path,
    report: Option<(&Path, &[u8])>,
    force: bool,
) -> Result<(), PublishError> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let texture_dir = output.with_file_name(texture_namespace(output)?);
    if !force
        && (output.exists()
            || texture_dir.exists()
            || report.is_some_and(|(path, _)| path.exists()))
    {
        return Err(PublishError::OutputExists);
    }

    let staging = tempfile::Builder::new()
        .prefix(".ice3-studio-asset-staging-")
        .tempdir_in(parent)?;
    let staged_textures = if bundle.resources.is_empty() {
        None
    } else {
        let path = staging.path().join(&bundle.texture_uri_prefix);
        std::fs::create_dir(&path)?;
        for resource in &bundle.resources {
            write_staged_resource(staging.path(), &bundle.texture_uri_prefix, resource)?;
        }
        Some(path)
    };
    let staged_glb = staging.path().join(".asset.glb");
    std::fs::write(&staged_glb, &bundle.glb)?;
    std::fs::File::open(&staged_glb)?.sync_all()?;
    let staged_report = report.map(|(_, bytes)| {
        let path = staging.path().join(".asset.report.json");
        std::fs::write(&path, bytes)?;
        std::io::Result::Ok(path)
    }).transpose()?;

    let backups = backup_publication_paths(output, &texture_dir, report)?;

    let publish_result = (|| {
        if let Some(staged) = staged_textures.as_ref() {
            std::fs::rename(staged, &texture_dir)?;
        }
        std::fs::rename(&staged_glb, output)?;
        if let (Some((report_path, _)), Some(staged)) = (report, staged_report.as_ref()) {
            std::fs::rename(staged, report_path)?;
        }
        Ok::<(), std::io::Error>(())
    })();

    if let Err(error) = publish_result {
        let mut details = Vec::new();
        restore_published_path(output, backups.output.as_deref(), &mut details);
        restore_published_path(&texture_dir, backups.textures.as_deref(), &mut details);
        if let Some((report_path, _)) = report {
            restore_published_path(report_path, backups.report.as_deref(), &mut details);
        }
        if details.is_empty() {
            return Err(PublishError::Io(error));
        }
        return Err(PublishError::RollbackFailed {
            primary: Box::new(PublishError::Io(error)),
            details,
        });
    }

    remove_optional_backup(output, backups.output.as_deref())?;
    remove_optional_backup(&texture_dir, backups.textures.as_deref())?;
    if let Some((report_path, _)) = report {
        remove_optional_backup(report_path, backups.report.as_deref())?;
    }
    Ok(())
}
```

`write_staged_resource` must reject absolute paths, parent components, root/prefix components,
empty file names and path collisions. Before writing, rehash bytes and compare to
`resource.digest`.

Add these private helpers with these exact signatures:

```rust
fn write_staged_resource(
    root: &Path,
    expected_namespace: &str,
    resource: &PreparedResource,
) -> Result<(), PublishError>;
fn backup_publication_paths(
    output: &Path,
    textures: &Path,
    report: Option<(&Path, &[u8])>,
) -> Result<PublicationBackups, PublishError>;
fn move_optional_path_to_backup(path: &Path) -> Result<Option<PathBuf>, PublishError>;
fn restore_published_path(path: &Path, backup: Option<&Path>, details: &mut Vec<String>);
fn remove_optional_backup(path: &Path, backup: Option<&Path>) -> Result<(), PublishError>;
```

Define `PublicationBackups { output, textures, report }` with `Option<PathBuf>` fields.
`backup_publication_paths` moves paths in texture, output, report order. If a later move fails,
it immediately restores all earlier backups before returning the error; callers must never use
`?` directly on three separate backup moves.

`move_optional_path_to_backup` must use `std::fs::rename` and preserve files and directories;
`restore_published_path` removes a partially published path only when it exists and then renames
the backup back. Every backup name is unique and lives beside the original target.
`write_staged_resource` requires `resource.uri` to start with the exact decoded
`expected_namespace + "/"`, strips that prefix, and writes the remainder beneath `root`; this
prevents both doubled namespaces and a URI that escapes the staged texture directory.

- [ ] **Step 5: Switch import and build to bundle preparation**

In both commands:

```rust
let canonical = to_canonical(&imported.scene);
let mut prepare_options = PrepareOptions {
    asset_id: options.asset_id.clone(),
    source_format: imported.scene.source_format.as_str().into(),
    source_digest: digest_label(&imported.source_digest),
    crease_angle_degrees: 30.0,
    generate_edges: false,
    budget: BudgetProfile::interactive_desktop_v1(),
    texture_namespace: texture_namespace(&options.output)
        .map_err(import_publish_error)?,
};
let bundle = prepare_canonical_bundle(&canonical, prepare_options)?;
publish_asset_bundle(&bundle, &options.output, None, options.force)
    .map_err(import_publish_error)?;
```

For build, use `generate_edges: !options.skip_edges`, preserve the requested budget and
crease angle, create report JSON from bundle metrics, and pass `Some((report_path, bytes))`
to the same publication function.

Map `PublishError::OutputExists` to the existing `ICE3_STUDIO_ASSET_OUTPUT_EXISTS`; map the
other variants to asset I/O with rollback details preserved.

- [ ] **Step 6: Add build transaction and no-clobber tests**

```rust
#[test]
fn build_publishes_textures_and_report_together() {
    let root = textured_gltf_fixture();
    let output = root.path().join("plant.glb");
    let report = root.path().join("plant.report.json");
    let result = cli(&[
        "build",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "plant",
        "--report",
        report.to_str().unwrap(),
    ]);
    assert_success(&result);
    assert!(output.is_file());
    assert!(report.is_file());
    assert!(root.path().join("plant.textures").is_dir());
}

#[test]
fn conflicting_texture_directory_blocks_without_writing_glb() {
    let root = textured_gltf_fixture();
    let output = root.path().join("plant.glb");
    std::fs::create_dir(root.path().join("plant.textures")).unwrap();
    std::fs::write(root.path().join("plant.textures/keep"), "keep").unwrap();

    let result = cli(&[
        "import",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "plant",
    ]);

    assert!(!result.status.success());
    assert!(!output.exists());
    assert_eq!(std::fs::read_to_string(root.path().join("plant.textures/keep")).unwrap(), "keep");
}
```

- [ ] **Step 7: Add an injected rollback unit test**

Factor the final rename sequence into:

```rust
fn publish_staged_with(
    staged_textures: &Path,
    texture_dir: &Path,
    staged_glb: &Path,
    output: &Path,
    staged_report: Option<(&Path, &Path)>,
    publish_report: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()>
```

The test pre-creates old GLB, old texture directory and old report, injects a report rename
failure, and asserts all three old paths are restored with their original contents. This test
pins compatibility with the existing prepare rollback behavior rather than trusting a
success-only path.

```rust
#[test]
fn failed_report_publish_restores_glb_and_texture_directory() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("plant.glb");
    let textures = root.path().join("plant.textures");
    let report = root.path().join("plant.report.json");
    let stage = tempfile::tempdir_in(root.path()).unwrap();
    let staged_glb = stage.path().join("plant.glb");
    let staged_textures = stage.path().join("plant.textures");
    let staged_report = stage.path().join("plant.report.json");
    std::fs::write(&output, "old-glb").unwrap();
    std::fs::create_dir(&textures).unwrap();
    std::fs::write(textures.join("old.png"), "old-texture").unwrap();
    std::fs::write(&report, "old-report").unwrap();
    std::fs::write(&staged_glb, "new-glb").unwrap();
    std::fs::create_dir(&staged_textures).unwrap();
    std::fs::write(staged_textures.join("new.png"), "new-texture").unwrap();
    std::fs::write(&staged_report, "new-report").unwrap();

    let result = publish_staged_with(
        &staged_textures,
        &textures,
        &staged_glb,
        &output,
        Some((&staged_report, &report)),
        |_, _| Err(std::io::Error::other("injected report failure")),
    );

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "old-glb");
    assert_eq!(std::fs::read_to_string(textures.join("old.png")).unwrap(), "old-texture");
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "old-report");
}
```

- [ ] **Step 8: Run CLI import/build tests**

Run: `cargo test -p studio-cli --test import`

Run: `cargo test -p studio-cli --test prepare_command`

Expected: PASS for textured sidecars, no-clobber, force replacement, report rollback and all
pre-existing untextured behavior.

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
git add crates/studio-cli/src crates/studio-cli/tests/import.rs crates/studio-cli/tests/prepare_command.rs
git commit -m "feat(cli): publish textured asset bundles"
```

### Task 11: Partition 纹理暂存与整目录发布

**Files:**
- Modify: `crates/studio-cli/src/partition.rs`
- Modify: `crates/studio-cli/tests/partition_command.rs`

**Interfaces:**
- Consumes: Task 9 `PreparedAssetBundle`；Task 10 publication helpers only where reusable。
- Produces: partition staging 中的 `parts/textures/<digest>.<ext>`；每个 tile 的相对 URI 可解析；整个 tileset 原子发布。

- [ ] **Step 1: Write the failing partition resource test**

```rust
#[test]
fn partition_publishes_textures_beside_part_glbs() {
    let root = textured_gltf_fixture();
    let output = root.path().join("dist");
    let result = run_cli(&[
        "partition",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "partition-textured",
        "--adapter",
        "gltf",
    ]);
    assert_success(&result);

    let part = first_part_glb(&output);
    let parsed = studio_assets::read_glb(&part).unwrap();
    let uri = parsed.json["images"][0]["uri"].as_str().unwrap();
    assert_eq!(uri, format!("textures/{}.png", digest_of_fixture_png()));
    assert!(output.join("parts").join(uri).is_file());
}
```

- [ ] **Step 2: Run and observe missing tile textures**

Run: `cargo test -p studio-cli --test partition_command partition_publishes_textures_beside_part_glbs`

Expected: FAIL because tiles reference `textures/...` but staging contains only GLBs and `tileset.json`.

- [ ] **Step 3: Prepare a bundle with tile-relative namespace**

```rust
let canonical = to_canonical(&imported.scene);
let source_digest = digest_label(&imported.source_digest);
let bundle = prepare_canonical_bundle(
    &canonical,
    PrepareOptions {
        asset_id: options.asset_id.clone(),
        source_format: imported.scene.source_format.as_str().into(),
        source_digest: source_digest.clone(),
        crease_angle_degrees: options.crease_angle_degrees,
        generate_edges: !options.skip_edges,
        budget,
        texture_namespace: "textures".into(),
    },
)?;
```

Write `bundle.glb` to the existing temporary Prepared GLB path, not `artifact.bytes`.

- [ ] **Step 4: Carry bundle resources into staging**

Change the staging signature:

```rust
fn write_staging(
    parent: &Path,
    artifact: &studio_pipeline::PartitionArtifact,
    resources: &[studio_assets::PreparedResource],
) -> Result<tempfile::TempDir, PartitionError> {
```

After writing tile files, call `write_texture_resources(staging.path(), resources)`.
Implement that helper with this signature and body:

```rust
fn write_texture_resources(
    staging_root: &Path,
    resources: &[studio_assets::PreparedResource],
) -> Result<(), PartitionError> {
    let mut seen = std::collections::BTreeMap::<String, &[u8]>::new();
    for resource in resources {
        let relative = Path::new(&resource.uri);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(PartitionError::Validation(
                "partition texture path escapes its output".into(),
            ));
        }
        if seen
            .insert(resource.uri.clone(), &resource.bytes)
            .is_some_and(|previous| previous != resource.bytes)
        {
            return Err(PartitionError::Validation(
                "partition texture URI has conflicting bytes".into(),
            ));
        }
        let path = staging_root.join("parts").join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &resource.bytes)?;
    }
    Ok(())
}
```

Keep the existing atomic `publish_directory` call unchanged. A resource write failure drops the
staging directory and never publishes `dist`.

- [ ] **Step 5: Add missing-resource and collision tests**

```rust
#[test]
fn texture_staging_rejects_escape_and_conflicting_duplicate_uris() {
    let root = tempfile::tempdir().unwrap();
    let escaped = studio_assets::PreparedResource {
        uri: "../outside.png".into(),
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![1],
    };
    let escape_error = write_texture_resources(root.path(), &[escaped]).unwrap_err();
    assert_eq!(escape_error.code(), "ICE3_STUDIO_ASSET_TILESET_INVALID");

    let first = studio_assets::PreparedResource {
        uri: "textures/same.png".into(),
        digest: "a".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![1],
    };
    let second = studio_assets::PreparedResource {
        uri: "textures/same.png".into(),
        digest: "b".repeat(64),
        media_type: "image/png".into(),
        bytes: vec![2],
    };
    let collision = write_texture_resources(root.path(), &[first, second]).unwrap_err();
    assert_eq!(collision.code(), "ICE3_STUDIO_ASSET_TILESET_INVALID");
}
```

- [ ] **Step 6: Run partition tests**

Run: `cargo test -p studio-cli --test partition_command`

Expected: PASS for textures beside tile GLBs, no-clobber, force directory replacement and all
existing untextured partitions.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/studio-cli/src/partition.rs crates/studio-cli/tests/partition_command.rs
git commit -m "feat(partition): publish material textures with tiles"
```

### Task 12: 跨命令验收、兼容基线与文档收口

**Files:**
- Create: `crates/studio-cli/tests/textured_material_fidelity.rs`
- Modify: `docs/specs/2026-11-20-import-material-surface-fidelity-design.md`
- Modify: `docs/README.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/reference/asset-pipeline.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `README.md`
- Modify: `benchmarks/prepare-baseline.json` only if the recorded baseline changes

**Interfaces:**
- Consumes: Tasks 1-11 的全部公开行为。
- Produces: 端到端材质验收、稳定 diagnostic 文档、Implemented 状态和完整门禁证据。

- [ ] **Step 1: Write the end-to-end acceptance test**

```rust
#[test]
fn imported_material_survives_import_and_build_with_all_resources() {
    let root = complete_surface_fixture();
    let import_output = root.path().join("imported.glb");
    let build_output = root.path().join("built.glb");

    run_success(&[
        "import",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        import_output.to_str().unwrap(),
        "--asset-id",
        "surface",
    ]);
    run_success(&[
        "build",
        root.path().join("surface.gltf").to_str().unwrap(),
        "--output",
        build_output.to_str().unwrap(),
        "--asset-id",
        "surface",
    ]);

    for output in [&import_output, &build_output] {
        let parsed = studio_assets::read_glb(output).unwrap();
        for uri in parsed.json["images"].as_array().unwrap() {
            assert!(output.parent().unwrap().join(uri["uri"].as_str().unwrap()).is_file());
        }
        let material = &parsed.json["materials"][0];
        assert_eq!(material["pbrMetallicRoughness"]["metallicFactor"], 0.8);
        assert_eq!(material["pbrMetallicRoughness"]["roughnessFactor"], 0.25);
        assert!(material["normalTexture"]["index"].is_number());
        assert!(material["occlusionTexture"]["index"].is_number());
    }
}
```

Add assertions for all four slots, transform, sampler, alpha mask, double-sided and exact
`TEXCOORD_0` values in both outputs.

- [ ] **Step 2: Run the acceptance test and fix gaps**

Run: `cargo test -p studio-cli --test textured_material_fidelity`

Expected: PASS only after every required material path is connected. Fix the owning task and
rerun its focused tests rather than weakening this acceptance test.

- [ ] **Step 3: Lock the legacy output baseline**

The canonical legacy golden digest `e3da988f73c5955b395b89d28268ff93a0004ddfd17def9a3765a66b24768d6d`
is locked in Task 6. Re-run that focused test here and do not add a CLI-source golden that mixes
the intended glTF PBR-default correction with canonical compatibility.

```bash
cargo test -p studio-assets --test canonical_writer canonical_material_defaults_keep_legacy_texture_behavior
```

Expected: PASS with the exact legacy canonical digest.

- [ ] **Step 4: Update diagnostics documentation**

Add exact code, trigger and recovery rows for:

```text
ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED
ICE3_STUDIO_IMPORT_TEXTURE_FORMAT_UNSUPPORTED
ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED
ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED
```

State that missing/escaping resources fail import and are not downgraded to warnings.

- [ ] **Step 5: Update the asset pipeline reference**

Document:

- glTF/GLB four texture slots and `TEXCOORD_0`;
- `plant.glb` plus `plant.textures/` and `parts/textures/`;
- content-addressed URIs and atomic publication;
- current non-goals and other import formats;
- `TEXCOORD_1+` warning behavior.

- [ ] **Step 6: Mark the implementation state only after all tests pass**

Change SPEC-034 and PLAN-031 front matter from `Proposed` to `Implemented`, update both index
rows in `docs/README.md`. Update
`docs/reference/capabilities.md` and `README.md` only where the verified current behavior
changed.

- [ ] **Step 7: Run format, lint and all Rust tests**

Run: `cargo fmt --all -- --check`

Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`

Run: `cargo test --workspace --all-features`

Expected: exit 0 with no warnings and no failing tests.

- [ ] **Step 8: Run documentation and schema gates**

Run: `node scripts/check-docs.mjs`

Run: `cargo run -p studio-cli -- schema --check`

Expected: documentation index complete; generated schema unchanged or intentionally updated and checked in.

- [ ] **Step 9: Run Prepare performance baseline**

Run: `npm run benchmark:prepare`

Expected: p95, peak RSS, bytes and digest gates pass. If and only if the machine-readable
baseline legitimately changes, review the diff, keep p95 within the documented limit, and
commit the updated `benchmarks/prepare-baseline.json`.

- [ ] **Step 10: Run the repository verification gate**

Run: `npm run verify`

Expected: all repository gates pass. Do not claim completion from a partial cargo run.

- [ ] **Step 11: Commit final acceptance and documentation**

```bash
cargo fmt --all
git add crates/studio-cli/tests/textured_material_fidelity.rs docs benchmarks README.md
git commit -m "test(import): verify material surface fidelity"
```

## Completion Checklist

- [ ] Source Surface, resource and UV contracts have serde defaults and Worker V2 coverage.
- [ ] External, data URI and bufferView images normalize to content-addressed resources.
- [ ] glTF PBR factors, four texture slots, alpha, double-sided, transform and sampler survive.
- [ ] `TEXCOORD_0` survives; unsupported UV channels warn and do not misbind.
- [ ] Canonical writer emits full surfaces, sampler-aware texture indices and conditional UVs.
- [ ] Prepared profile validates all slots, transform, sampler and relative resource URIs.
- [ ] Import and build atomically publish GLB, textures and report.
- [ ] Partition atomically publishes tile GLBs and `parts/textures/`.
- [ ] Untextured historical output remains byte-identical.
- [ ] Diagnostics, asset pipeline, capabilities and indexes match implemented code.
- [ ] Format, lint, workspace tests, schema, docs, Prepare benchmark and `npm run verify` pass.
