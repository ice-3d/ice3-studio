# 导入材质表面保真设计

> Document ID: ICE3-STUDIO-SPEC-034
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

Studio 的 Document 表面模型和 Prepared GLB 已经支持底色、法线、ORM、PBR 因子与
UV transform，但 Source Import 只读取基础色和透明度。glTF/GLB 中的纹理表、图片、
法线贴图、金属粗糙度贴图、遮蔽贴图、材质因子和源 UV 都会丢失。

本设计交付第一阶段完整闭环：

- 从 glTF/GLB 导入金属粗糙度材质和 `TEXCOORD_0`；
- 保留 base-color、normal、occlusion 和 metallic-roughness 贴图；
- 保留 PBR 因子、法线强度、UV transform、sampler、alpha 和双面设置；
- 把外部图片、data URI 和 GLB 内嵌图片归一化为受控资源；
- 由 `studio-assets::prepare` 生成引用相对路径纹理的 Prepared GLB；
- 让 `import`、`build` 和 `partition` 一起发布 GLB 与图片文件；
- 对路径逃逸、超限资源和无法正确表达的特性失败关闭或产生确定性 warning。

成功标准是：带材质的源 glTF/GLB 经 Studio 导入后，无需在下游重新绑定材质，就能得到
与原表面槽位一致的 Prepared GLB 和可用纹理文件。

## 2. 范围

### 2.1 第一阶段

- glTF 2.0 JSON 与 GLB；
- `TEXCOORD_0`；
- metallic-roughness PBR 材质；
- base-color、normal、occlusion 和 metallic-roughness 四个贴图槽；
- `KHR_texture_transform`；
- sampler 的 wrap 与 filter；
- `OPAQUE`、`MASK`、`BLEND`；
- `doubleSided`；
- 外部相对图片、data URI、GLB 内嵌图片；
- Source Import、Canonical Prepare、Import CLI、Build CLI 和 Partition CLI。

### 2.2 非目标

- `TEXCOORD_1` 及更高 UV 集；
- sparse accessor、非 VEC2 UV 和未归一化的整数 UV；
- 压缩纹理解码或 GPU 纹理转码；
- emissive texture；
- clearcoat、transmission、volume、iridescence、sheen 等材质扩展；
- 自动重采样、通道拆分或 ORM 通道重打包；
- OBJ/MTL、FBX、USD、STEP、IFC 和 JT 的纹理支持；
- 视口材质编辑器变更。

其它 Source Import 格式在本阶段继续保留已有 warning。它们复用本设计的数据契约，但
分别通过后续设计接入，避免把格式适配与共享资源安全混在一次交付中。

## 3. 术语

| 术语 | 含义 |
|---|---|
| Surface | 材质的物理表面参数与贴图槽集合 |
| texture binding | 贴图资源、UV 通道、transform 与 sampler 的组合 |
| resource | 经过安全和格式校验的图片字节，以内容摘要标识 |
| bundle | Prepared GLB 字节与其相对路径资源的集合 |
| texture namespace | 输出 GLB 相对的文件目录，例如 `plant.textures/` |

## 4. 数据契约

### 4.1 Source 材质

`SourceMaterial` 增加以下可选表面和材质状态：

```rust
pub struct SourceMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub opacity: f32,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    pub surface: Option<SourceSurface>,
    pub extras: Value,
}

pub struct SourceSurface {
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: Option<[f32; 3]>,
    pub base_color_texture: Option<SourceTextureBinding>,
    pub normal_texture: Option<SourceNormalTexture>,
    pub occlusion_texture: Option<SourceTextureBinding>,
    pub metallic_roughness_texture: Option<SourceTextureBinding>,
}

pub struct SourceNormalTexture {
    pub binding: SourceTextureBinding,
    pub scale: f32,
}

pub struct SourceTextureBinding {
    pub resource: String,
    pub tex_coord: u32,
    pub transform: TextureTransform,
    pub sampler: TextureSampler,
}

pub struct TextureTransform {
    pub offset: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

pub struct TextureSampler {
    pub mag_filter: Option<u32>,
    pub min_filter: Option<u32>,
    pub wrap_s: u32,
    pub wrap_t: u32,
}
```

缺省值：

| 字段 | 缺省 |
|---|---|
| `alpha_mode` | `OPAQUE` |
| `alpha_cutoff` | `0.5` |
| `double_sided` | `false` |
| `surface` | `None` |
| `metallic` | `1.0`，按 glTF 材质缺省 |
| `roughness` | `1.0`，按 glTF 材质缺省 |
| `normal.scale` | `1.0` |
| transform offset | `[0, 0]` |
| transform rotation | `0` |
| transform scale | `[1, 1]` |
| sampler wrap | `REPEAT` |
| sampler filter | 未指定 |

`surface=None` 不改变历史材质调用行为。glTF 对 metallic-roughness 有明确的缺省值，
因此 glTF 适配器会为每个材质构造 `Some`；其它适配器只有在源格式实际提供表面信息时才
构造 `Some`。

### 4.2 Source 资源

`SourceScene` 增加资源表：

```rust
pub struct SourceResource {
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}
```

`digest` 是图片原始字节 SHA-256 的小写 64 位十六进制字符串，不带算法前缀。binding 的
`resource` 引用该摘要。资源表按摘要排序，相同字节只保留一项。图片字节在 Worker JSON
中使用 base64 表示，并纳入场景预算。

### 4.3 UV

`SourcePrimitive` 与 `CanonicalPrimitive` 增加：

```rust
pub uvs: Option<Vec<[f32; 2]>>,
```

它对应 `TEXCOORD_0`。第一阶段接受 FLOAT VEC2，以及 normalized UBYTE/USHORT VEC2；
导入时统一解码为 FLOAT VEC2。出现时，长度必须等于 `positions`，数值必须有限。Sparse
accessor 或其它编码产生 `ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED`，并跳过
依赖该 UV 的贴图。Canonical writer 优先写入解码后的 `TEXCOORD_0`；没有 UV 时保留现有
按主导轴盒投影的兜底行为，使没有任何 UV 的历史资产和程序化几何仍然可用。

### 4.4 Canonical 材质与兼容

`CanonicalMaterial` 增加：

```rust
pub alpha_mode: AlphaMode,
pub alpha_cutoff: f32,
pub double_sided: bool,
pub surface: Option<CanonicalSurface>,
```

`CanonicalSurface` 的四个槽位与 Source 一致，但 binding 使用资源摘要。现有
`CanonicalMaterial.texture` 保留为 base-color 兼容入口：

- `texture` 存在且 `surface.base_color_texture` 为空时，等同旧行为；
- 两者指向同一等效贴图时只输出一次；
- 两者同时指定不同 base-color 贴图或不同 transform 时返回 profile error；
- 无新 surface 的调用仍生成原有材质 JSON。

现有 `orm_texture` 简写继续表示同一个资源同时用于 occlusion 和
metallic-roughness。CanonicalSurface 可以分别设置两个槽；只有资源、UV 通道、transform
和 sampler 全部一致，且 sampler 为缺省、transform 只有 `uv_scale` 时才允许写入
`orm_texture` 简写。无法由旧 shape 无损表达的绑定使用独立的 CanonicalSurface 槽位。

## 5. glTF/GLB 映射

### 5.1 材质

| glTF 字段 | Studio 字段 |
|---|---|
| `pbrMetallicRoughness.baseColorFactor` | `base_color`、`opacity` |
| `pbrMetallicRoughness.metallicFactor` | `surface.metallic` |
| `pbrMetallicRoughness.roughnessFactor` | `surface.roughness` |
| `pbrMetallicRoughness.baseColorTexture` | `surface.base_color_texture` |
| `pbrMetallicRoughness.metallicRoughnessTexture` | `surface.metallic_roughness_texture` |
| `normalTexture` | `surface.normal_texture` |
| `normalTexture.scale` | `surface.normal_texture.scale` |
| `occlusionTexture` | `surface.occlusion_texture` |
| `emissiveFactor` | `surface.emissive` |
| `alphaMode` | `alpha_mode` |
| `alphaCutoff` | `alpha_cutoff` |
| `doubleSided` | `double_sided` |

缺省值严格采用 glTF 2.0 语义。颜色和因子必须有限；因子范围、alpha cutoff 和法线强度
在进入 Canonical 前校验。

### 5.2 贴图解析

每个 textureInfo 依次解析：

1. `textureInfo.index → textures[index]`；
2. `texture.source → images[imageIndex]`；
3. `texture.sampler → samplers[samplerIndex]`；
4. `textureInfo.texCoord`，缺省为 0；
5. `textureInfo.extensions.KHR_texture_transform`；
6. image 的 `uri` 或 `bufferView + mimeType`。

任一索引越界、图片缺失、引用不可读或媒体内容与声明不符，均使导入失败。缺失资源不能
降级为“成功但少一张图”。

所有槽位共享资源表，但各自保留 texCoord、transform 和 sampler。一个图片用于多个槽位
时复用同一个 digest；不会因为上游把 occlusion 与 metallic-roughness 指向同一文件而
复制字节。

Canonical writer 分别对 images 按 digest、samplers 按完整 sampler 值、textures 按
`(image index, sampler index)` 做稳定去重。不同 sampler 不能复用同一个 texture index。

### 5.3 UV transform

`KHR_texture_transform` 的 `offset`、`rotation` 和 `scale` 原值保留并写入 Prepared GLB。
扩展内的 `texCoord` 若存在，覆盖 textureInfo 的通道。

第一阶段只输出 `TEXCOORD_0`。binding 的有效 `tex_coord` 非零时：

- 产生 `ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED`；
- 省略该贴图槽；
- 保留材质因子和其它可表达槽位。

不得把高通道贴图错误绑定到 `TEXCOORD_0`。

### 5.4 图片来源

支持：

- 外部相对 URI；
- base64 或百分号编码的 data URI；
- GLB `bufferView` 内嵌图片。

三类来源都进入统一受阻读取路径，并最终生成 `SourceResource`。GLB 内嵌图片成功导入后，
不再产生“图片被忽略”的 warning。

PNG、JPEG 和 WebP 由文件头识别。`mimeType` 只作为一致性校验，不作为信任来源。

### 5.5 降级与失败

新增稳定 warning：

| Code | 触发条件 |
|---|---|
| `ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED` | 贴图绑定到 `TEXCOORD_1+` |
| `ICE3_STUDIO_IMPORT_TEXTURE_FORMAT_UNSUPPORTED` | 可选贴图使用不支持的格式 |
| `ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED` | emissive texture 等未支持槽位 |
| `ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED` | 材质扩展未进入第一阶段 |

`extensionsRequired` 中声明了本阶段无法读取的纹理扩展时，导入失败。未知的
`extensionsUsed` 只产生 warning。

路径越界、资源超出预算、摘要冲突、文件缺失声明和损坏结构使用现有
`ICE3_STUDIO_IMPORT_INVALID_SOURCE` 或 `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED`，不新增含糊
的“部分成功”状态。

## 6. 资源安全

### 6.1 受限读取

外部资源读取必须：

- 拒绝绝对路径、协议 URL、`..` 与根目录组件；
- 对解码后的 URI 再次检查，避免百分号编码绕过；
- canonicalize 目标和源目录，确认目标位于源目录内；
- 使用 `symlink_metadata` 拒绝符号链接目标和逃逸链接；
- 在读取前检查文件大小，不使用无界 `read`。

data URI 在解码前按编码长度估算输出上限。GLB `bufferView` 使用声明的 byte range，
检查范围、声明长度和实际 buffer 长度。

### 6.2 预算

| 项 | 上限 |
|---|---:|
| 单个图片字节 | 64 MiB |
| 图片总字节 | 256 MiB |
| 图片数量 | 4096 |
| SourceScene 总估算内存 | `max_memory_bytes` |

图片预算与现有节点、三角形和场景字节预算共同检查。资源字节、摘要、媒体类型、URI
字符串和 Worker base64 开销进入 `estimated_scene_bytes`。

预算检查发生在完整读取、复制或编码之前，失败时不得留下输出。

### 6.3 格式校验

允许：

| 媒体类型 | 扩展名 | 文件头 |
|---|---|---|
| PNG | `.png` | PNG signature |
| JPEG | `.jpg` | JPEG SOI |
| WebP | `.webp` | RIFF + WEBP |

SVG、HTML、未知二进制和伪造扩展名均拒绝。输出 URI 不使用源文件名，只使用摘要和由
媒体类型决定的扩展名。

## 7. Prepared bundle 与发布

### 7.1 Bundle 契约

新增：

```rust
pub struct PreparedAssetBundle {
    pub glb: Vec<u8>,
    pub resources: Vec<PreparedResource>,
}

pub struct PreparedResource {
    pub uri: String,
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}
```

`studio-assets::prepare` 仍是唯一准备写出器。现有只返回 GLB 字节的入口继续支持无资源
调用；带资源的 CLI 链路使用 bundle。bundle 不启动进程、不调用外部转换器。

### 7.2 输出布局

`import` 和 `build` 使用以输出文件名为前缀的纹理目录。物理目录保留输出文件 stem，
GLB URI 对每个路径段做百分号编码并使用 `/` 分隔：

```text
plant.glb
plant.textures/
  6f6f...b7.png
  9a31...20.jpg
```

GLB 中写入：

```json
{
  "images": [
    {
      "uri": "plant.textures/6f6f...b7.png",
      "mimeType": "image/png"
    }
  ]
}
```

URI 必须相对 GLB 所在目录，并经过与现有 Prepared profile 相同的拒绝规则。旧调用若
只提供外部 URI 而不提供资源字节，writer 保持旧行为，不声称 bundle 自包含。

### 7.3 原子发布

`import` 和 `build` 的发布按以下顺序：

1. 在输出父目录创建唯一 staging 目录和临时 GLB。
2. 写入并 `fsync` 所有纹理。
3. 校验目标冲突、report 路径和 `--force` 事务边界。
4. 在需要覆盖时，把旧 GLB、旧纹理目录和旧 report 移到唯一备份位置。
5. 发布新纹理目录。
6. 最后原子替换 GLB，再发布 report。
7. 任一步失败时按相反顺序恢复全部旧路径，并清除本次新建的临时路径。

非 `--force` 模式下，任一目标已存在就失败，不能先写一张图片再报告输出冲突。

### 7.4 Partition

Partition 已在暂存目录中生成完整 tileset。资源写入：

```text
parts/
  part-000.glb
  textures/
    6f6f...b7.png
```

每个 tile 的 `images[].uri` 指向 `textures/<digest>.<ext>`，相对 tile GLB 解析正确。
缺失资源、URI 逃逸或 digest 不一致使整个 partition 失败，整个 tileset 不发布。

## 8. 代码边界

| 位置 | 责任 |
|---|---|
| `crates/studio-import/src/source.rs` | Surface、binding、resource 与 UV 数据契约 |
| `crates/studio-import/src/gltf.rs` | 材质、texture、image、sampler、transform 和 UV 解析 |
| `crates/studio-import/src/limits.rs` | 图片与资源预算 |
| `crates/studio-import/src/worker.rs` | 协议版本 2 与资源载荷 |
| `crates/studio-import/src/lib.rs` | Source 到 Canonical 的完整转换 |
| `crates/studio-assets/src/glb.rs` | 有界 URI、data URI 与 bufferView 读取 |
| `crates/studio-assets/src/canonical.rs` | TEXCOORD、全部材质槽、资源 URI 和 bundle |
| `crates/studio-assets/src/prepared.rs` | 四个贴图槽、sampler、transform 和 URI 校验 |
| `crates/studio-cli/src/import.rs` | import bundle 发布 |
| `crates/studio-cli/src/prepare_command.rs` | build bundle、report 与事务 |
| `crates/studio-cli/src/partition.rs` | partition 纹理暂存与原子目录发布 |

资源读取、图片格式识别和内容摘要分别封装为小接口。glTF 适配器只负责标准字段映射，
不直接拼接文件路径，也不执行文件复制。

## 9. 测试

### 9.1 契约测试

- 新字段的 serde 缺省值；
- 旧材质 JSON 仍能反序列化；
- Worker 协议 2 往返包含图片资源；
- Source 到 Canonical 的每个槽位、UV 和 sampler 映射；
- 旧 `CanonicalMaterial.texture` 行为不变；
- 冲突的 legacy texture 与 surface 返回 profile error。

### 9.2 glTF 测试

使用最小确定性 fixture 覆盖：

- 外部 PNG base-color；
- data URI JPEG normal；
- GLB bufferView WebP；
- 独立 occlusion 与 metallic-roughness；
- 共享 ORM 图片；
- 四个槽不同 texCoord、transform 和 sampler；
- `TEXCOORD_0` 原值与无 UV 时的盒投影；
- `MASK`、`BLEND`、alpha cutoff 与 double-sided；
- 非零 texCoord、压缩纹理和 emissive texture 的确定性 warning。

### 9.3 安全测试

- 绝对路径、协议 URL、`..`、百分号编码目录逃逸；
- 指向源目录外的符号链接；
- 缺失图片、越界 bufferView 和错误 mimeType；
- 超过单图、总量、数量和场景预算；
- 伪造扩展名、SVG、HTML 和未知二进制；
- 相同摘要不同字节、相同摘要不同媒体类型。

### 9.4 CLI 端到端

- `import` 后 GLB 的所有纹理 URI 均存在且摘要匹配；
- `build` 成功时 GLB、纹理目录与 report 一起出现；
- `partition` 的每个 tile 在 `parts/textures/` 找到同一资源；
- 非 force 冲突不写任何新文件；
- 强制发布故障注入后旧 GLB、旧纹理目录与旧 report 全部恢复；
- 无 surface fixture 的输出与变更前 golden digest 一致。

## 10. 验收

- base-color、normal、occlusion、metallic-roughness 四个槽在 Prepared GLB 中可解析；
- metallic、roughness、normal scale、emissive、alpha 和 double-sided 原值保留；
- `TEXCOORD_0` 与 transform、sampler 原值保留；
- 外部、data URI、内嵌图片都产生可复现的相对路径资源；
- 输出目录在 GLB 出现前已包含全部所需图片；
- 任一路径逃逸或预算超限都失败且没有部分输出；
- 无贴图资产保持 byte-identical；
- 诊断文档、资产流水线文档和文档索引同步更新；
- Rust format、lint、workspace 测试、schema、文档检查和根级 `npm run verify` 通过。

## 11. 后续格式

第一阶段完成后，按以下顺序复用共享契约：

1. OBJ/MTL：基础色、法线、粗糙度和外部图片；
2. FBX：材质槽、UV 和隔离 Worker 资源载荷；
3. USD：UV、纹理与有限材质网络；
4. STEP/IFC：仅在其表达可无损映射时接入表面纹理，否则继续明确 warning。

每个格式单独形成设计或实施范围，不因本设计完成而自动视为支持。

## 12. 关联文档

- `ICE3-STUDIO-SPEC-016` — [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
- `ICE3-STUDIO-SPEC-032` — [材质表面、UV 与 PBR 导出](2026-11-04-material-pbr-uv-design.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](../reference/diagnostics.md)
- `ICE3-STUDIO-REF-005` — [资产流水线](../reference/asset-pipeline.md)
