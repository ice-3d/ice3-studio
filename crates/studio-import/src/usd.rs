use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use glam::{DMat4, DQuat, DVec3};
use openusd::ar::{AssetInfo, DefaultResolver, ResolvedPath, Resolver};
use openusd::gf::Matrix4d;
use openusd::sdf::{self, FieldKey, SpecType, Value as SdfValue};
use openusd::tf::Token;
use openusd::usd::{Prim, PrimPredicate, Stage};
use serde_json::{Map, Value, json};

use crate::{
    ImportError, ImportRequest, ImportWarning, NativeImportLimits, SourceCapabilities,
    SourceFormat, SourceImporter, SourceMaterial, SourceMesh, SourceNode, SourcePrimitive,
    SourceScene, validate_scene_limits,
};

const MAX_COMPOSITION_DEPTH: usize = 32;
const MAX_COMPOSITION_LAYERS: usize = 1_024;
const MAX_METADATA_ARRAY_LENGTH: usize = 256;
const MAX_METADATA_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
pub struct UsdImporter;

impl UsdImporter {
    pub const fn new() -> Self {
        Self
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import_file_with_limits(path, asset_id, NativeImportLimits::default())
    }

    pub fn import_file_with_limits(
        &self,
        path: &Path,
        asset_id: &str,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        self.import_with_limits(
            &ImportRequest {
                path: path.to_path_buf(),
                asset_id: asset_id.into(),
                requested_adapter: Some("usd".into()),
            },
            limits,
        )
    }

    pub fn import_with_limits(
        &self,
        request: &ImportRequest,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        let asset_id = request.asset_id.trim();
        if asset_id.is_empty() {
            return Err(ImportError::InvalidSource(
                "asset ID must not be empty".into(),
            ));
        }
        let root = request
            .path
            .canonicalize()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let source_directory = root
            .parent()
            .ok_or_else(|| ImportError::InvalidSource("USD root has no directory".into()))?
            .canonicalize()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let composition = inspect_composition(&root, &source_directory, limits)?;
        let errors = Arc::new(Mutex::new(Vec::new()));
        let resolver =
            ConfinedResolver::new(source_directory.clone(), root.clone(), Arc::clone(&errors));
        let root_text = root
            .to_str()
            .ok_or_else(|| ImportError::InvalidSource("USD path is not valid UTF-8".into()))?;
        let stage = Stage::builder()
            .resolver(resolver)
            .open(root_text)
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let mut paths = Vec::new();
        stage
            .traverse(PrimPredicate::DEFAULT_PROXIES, |path| {
                paths.push(path.clone());
            })
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        paths.dedup();
        if let Some(error) = lock(&errors).first().cloned() {
            return Err(ImportError::InvalidSource(error));
        }
        if !stage.composition_errors().is_empty() {
            return Err(ImportError::InvalidSource(
                "USD composition contains unresolved or cyclic arcs".into(),
            ));
        }
        if paths.len() > limits.max_nodes {
            return Err(ImportError::LimitExceeded(format!(
                "USD has {} prims; limit is {}",
                paths.len(),
                limits.max_nodes
            )));
        }
        let scene = import_stage(&stage, &paths, &root, asset_id, limits, &composition)?;
        if let Some(error) = lock(&errors).first().cloned() {
            return Err(ImportError::InvalidSource(error));
        }
        Ok(scene)
    }
}

impl SourceImporter for UsdImporter {
    fn id(&self) -> &'static str {
        "usd"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".usda", ".usdc"]
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: true,
            material_colors: true,
            instances: true,
            semantic_metadata: true,
        }
    }

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        self.import_with_limits(request, NativeImportLimits::default())
    }
}

#[derive(Debug, Clone, Copy)]
struct CompositionStats {
    layers: usize,
    depth: usize,
}

fn inspect_composition(
    root: &Path,
    source_directory: &Path,
    limits: NativeImportLimits,
) -> Result<CompositionStats, ImportError> {
    let mut state = CompositionScan {
        source_directory,
        limits,
        visited: BTreeSet::new(),
        visiting: BTreeSet::new(),
        layer_count: 0,
        deepest: 0,
        total_bytes: 0,
    };
    state.visit(root, 1)?;
    Ok(CompositionStats {
        layers: state.layer_count,
        depth: state.deepest,
    })
}

struct CompositionScan<'a> {
    source_directory: &'a Path,
    limits: NativeImportLimits,
    visited: BTreeSet<PathBuf>,
    visiting: BTreeSet<PathBuf>,
    layer_count: usize,
    deepest: usize,
    total_bytes: usize,
}

impl CompositionScan<'_> {
    fn visit(&mut self, path: &Path, depth: usize) -> Result<(), ImportError> {
        if depth > MAX_COMPOSITION_DEPTH {
            return Err(ImportError::InvalidSource(format!(
                "USD composition depth exceeds {MAX_COMPOSITION_DEPTH}"
            )));
        }
        let canonical = path
            .canonicalize()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        ensure_inside(self.source_directory, &canonical)?;
        if self.visiting.contains(&canonical) {
            return Err(ImportError::InvalidSource(
                "USD composition contains a cycle".into(),
            ));
        }
        if !self.visited.insert(canonical.clone()) {
            return Ok(());
        }
        self.visiting.insert(canonical.clone());
        self.layer_count += 1;
        if self.layer_count > MAX_COMPOSITION_LAYERS {
            return Err(ImportError::InvalidSource(format!(
                "USD composition exceeds {MAX_COMPOSITION_LAYERS} layers"
            )));
        }
        self.deepest = self.deepest.max(depth);
        let metadata = std::fs::metadata(&canonical)?;
        let size = usize::try_from(metadata.len()).map_err(|_| {
            ImportError::LimitExceeded("USD layer size does not fit the platform".into())
        })?;
        if size > self.limits.max_memory_bytes {
            return Err(ImportError::LimitExceeded(format!(
                "USD layer has {size} bytes; limit is {}",
                self.limits.max_memory_bytes
            )));
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(size)
            .ok_or_else(|| ImportError::LimitExceeded("USD layer bytes overflowed".into()))?;
        if self.total_bytes > self.limits.max_memory_bytes {
            return Err(ImportError::LimitExceeded(format!(
                "USD composition has {} bytes; limit is {}",
                self.total_bytes, self.limits.max_memory_bytes
            )));
        }

        let layer = sdf::Layer::open(canonical.to_str().ok_or_else(|| {
            ImportError::InvalidSource("USD layer path is not valid UTF-8".into())
        })?)
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let dependencies = layer_dependencies(&layer)?;
        for dependency in dependencies {
            let resolved = resolve_dependency(&canonical, self.source_directory, &dependency)?;
            self.visit(&resolved, depth + 1)?;
        }
        self.visiting.remove(&canonical);
        Ok(())
    }
}

fn layer_dependencies(layer: &sdf::Layer) -> Result<BTreeSet<String>, ImportError> {
    let mut dependencies = BTreeSet::new();
    let root = sdf::path("/").map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    if let Ok(Some(value)) = layer.data().try_field(&root, FieldKey::SubLayers.as_str()) {
        add_string_dependencies(&value, &mut dependencies);
    }
    for path in layer.data().spec_paths() {
        if layer.data().spec_type(&path) != Some(SpecType::Prim) {
            continue;
        }
        for field in [FieldKey::References.as_str(), FieldKey::Payload.as_str()] {
            if let Ok(Some(value)) = layer.data().try_field(&path, field) {
                add_arc_dependencies(&value, &mut dependencies);
            }
        }
    }
    Ok(dependencies)
}

fn add_string_dependencies(value: &SdfValue, output: &mut BTreeSet<String>) {
    match value {
        SdfValue::StringVec(values) => output.extend(values.iter().cloned()),
        SdfValue::StringListOp(values) => output.extend(values.flatten()),
        _ => {}
    }
}

fn add_arc_dependencies(value: &SdfValue, output: &mut BTreeSet<String>) {
    match value {
        SdfValue::ReferenceListOp(items) => output.extend(
            items
                .flatten()
                .into_iter()
                .map(|item| item.asset_path)
                .filter(|path| !path.is_empty()),
        ),
        SdfValue::PayloadListOp(items) => output.extend(
            items
                .flatten()
                .into_iter()
                .map(|item| item.asset_path)
                .filter(|path| !path.is_empty()),
        ),
        SdfValue::Payload(item) if !item.asset_path.is_empty() => {
            output.insert(item.asset_path.clone());
        }
        _ => {}
    }
}

fn resolve_dependency(
    referring_layer: &Path,
    source_directory: &Path,
    authored: &str,
) -> Result<PathBuf, ImportError> {
    validate_authored_asset_path(authored)?;
    let normalized = authored.replace('\\', "/");
    let dependency = Path::new(&normalized);
    if dependency.is_absolute() {
        return Err(ImportError::InvalidSource(
            "USD references must be relative to the source directory".into(),
        ));
    }
    let candidate = referring_layer
        .parent()
        .ok_or_else(|| ImportError::InvalidSource("USD layer has no parent".into()))?
        .join(dependency);
    let canonical = candidate.canonicalize().map_err(|error| {
        ImportError::InvalidSource(format!(
            "USD composition dependency {authored:?} is missing: {error}"
        ))
    })?;
    ensure_inside(source_directory, &canonical)?;
    Ok(canonical)
}

fn validate_authored_asset_path(authored: &str) -> Result<(), ImportError> {
    let normalized = authored.replace('\\', "/");
    let scheme = normalized
        .find(':')
        .is_some_and(|colon| !normalized[..colon].contains('/'));
    if scheme
        || normalized.starts_with('/')
        || normalized.split('/').any(|component| component == "..")
        || normalized.as_bytes().get(1) == Some(&b':')
    {
        return Err(ImportError::InvalidSource(format!(
            "USD asset path {authored:?} escapes the source directory"
        )));
    }
    Ok(())
}

fn ensure_inside(root: &Path, candidate: &Path) -> Result<(), ImportError> {
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(ImportError::InvalidSource(
            "USD asset resolves outside the source directory".into(),
        ))
    }
}

struct ConfinedResolver {
    inner: DefaultResolver,
    source_directory: PathBuf,
    root_file: PathBuf,
    errors: Arc<Mutex<Vec<String>>>,
}

impl ConfinedResolver {
    fn new(source_directory: PathBuf, root_file: PathBuf, errors: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inner: DefaultResolver::with_search_paths([source_directory.clone()]),
            source_directory,
            root_file,
            errors,
        }
    }

    fn record(&self, error: impl Into<String>) {
        lock(&self.errors).push(error.into());
    }

    fn check_path(&self, path: &Path) -> bool {
        let Ok(canonical) = path.canonicalize() else {
            return path.starts_with(&self.source_directory) || path == self.root_file;
        };
        if canonical == self.root_file || canonical.starts_with(&self.source_directory) {
            true
        } else {
            self.record("USD asset resolves outside the source directory");
            false
        }
    }
}

impl Resolver for ConfinedResolver {
    fn create_identifier(&self, asset_path: &str, anchor: Option<&ResolvedPath>) -> String {
        let candidate = if anchor.is_none()
            && Path::new(asset_path)
                .canonicalize()
                .is_ok_and(|path| path == self.root_file)
        {
            Ok(())
        } else {
            validate_authored_asset_path(asset_path)
        };
        if let Err(error) = candidate {
            self.record(error.to_string());
        }
        let identifier = self.inner.create_identifier(asset_path, anchor);
        if !identifier.is_empty() {
            let _ = self.check_path(Path::new(&identifier));
        }
        identifier
    }

    fn resolve(&self, asset_path: &str) -> Option<ResolvedPath> {
        let resolved = self.inner.resolve(asset_path)?;
        self.check_path(&resolved).then_some(resolved)
    }

    fn resolve_for_new_asset(&self, asset_path: &str) -> Option<ResolvedPath> {
        let resolved = self.inner.resolve_for_new_asset(asset_path)?;
        self.check_path(&resolved).then_some(resolved)
    }

    fn open_asset(&self, resolved_path: &ResolvedPath) -> io::Result<Box<dyn openusd::ar::Asset>> {
        if !self.check_path(resolved_path) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "asset is outside the USD source directory",
            ));
        }
        self.inner.open_asset(resolved_path)
    }

    fn is_context_dependent_path(&self, asset_path: &str) -> bool {
        self.inner.is_context_dependent_path(asset_path)
    }

    fn in_memory_identifier<'a>(&self, asset_path: &'a str) -> Cow<'a, str> {
        self.inner.in_memory_identifier(asset_path)
    }

    fn get_asset_info(&self, asset_path: &str, resolved_path: &ResolvedPath) -> AssetInfo {
        self.inner.get_asset_info(asset_path, resolved_path)
    }

    fn identity(&self) -> String {
        format!("confined:{}", self.source_directory.display())
    }

    fn cache_id(&self) -> Option<u64> {
        self.inner.cache_id()
    }
}

fn import_stage(
    stage: &Stage,
    paths: &[sdf::Path],
    root_file: &Path,
    asset_id: &str,
    limits: NativeImportLimits,
    composition: &CompositionStats,
) -> Result<SourceScene, ImportError> {
    let meters_per_unit = stage_number(stage, "metersPerUnit").unwrap_or(1.0);
    if !meters_per_unit.is_finite() || meters_per_unit <= 0.0 {
        return Err(ImportError::InvalidSource(
            "USD metersPerUnit must be finite and positive".into(),
        ));
    }
    let up_axis = stage_token(stage, "upAxis").unwrap_or_else(|| "Y".into());
    let handedness = stage_token(stage, "handedness").unwrap_or_else(|| "right".into());
    let conversion = coordinate_conversion(meters_per_unit, &up_axis, &handedness)?;

    let materials = import_materials(stage, paths, asset_id)?;
    let material_by_path = materials
        .iter()
        .enumerate()
        .map(|(index, (path, _))| (path.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let material_list = materials
        .into_iter()
        .map(|(_, material)| material)
        .collect::<Vec<_>>();

    let mut warnings = BTreeSet::new();
    add_resource_warnings(stage, paths, &mut warnings)?;
    let point_instancers = collect_point_instancers(stage, paths)?;
    let skipped_prototypes = point_instancers
        .iter()
        .flat_map(|item| item.prototypes.iter().cloned())
        .collect::<BTreeSet<_>>();

    let mesh_paths = paths
        .iter()
        .filter(|path| prim_type(stage, path).as_deref() == Some("Mesh"))
        .cloned()
        .collect::<Vec<_>>();
    let mut mesh_keys = BTreeSet::new();
    let mut first_mesh_for_key = BTreeMap::<String, Prim>::new();
    for path in &mesh_paths {
        let prim = stage
            .prim(path.clone())
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let key = mesh_identity(&prim, path)?;
        mesh_keys.insert(key.clone());
        first_mesh_for_key.entry(key).or_insert(prim);
    }
    let instance_groups = mesh_keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), format!("usd/instance/mesh/{index}")))
        .collect::<BTreeMap<_, _>>();
    let mut meshes = Vec::with_capacity(mesh_keys.len());
    let mut mesh_indices = BTreeMap::new();
    let mut triangle_total = 0_u64;
    for (mesh_index, key) in mesh_keys.into_iter().enumerate() {
        let prim = first_mesh_for_key.get(&key).ok_or_else(|| {
            ImportError::InvalidSource("USD mesh identity has no source prim".into())
        })?;
        let material = bound_material(prim, &material_by_path)?;
        let (mesh, triangles) = import_mesh(prim, &key, mesh_index, asset_id, material)?;
        triangle_total = triangle_total
            .checked_add(triangles)
            .ok_or_else(|| ImportError::LimitExceeded("USD triangle count overflowed".into()))?;
        if triangle_total > limits.max_triangles {
            return Err(ImportError::LimitExceeded(format!(
                "USD has {triangle_total} triangles; limit is {}",
                limits.max_triangles
            )));
        }
        meshes.push(mesh);
        mesh_indices.insert(key, mesh_index);
    }
    if triangle_total == 0 {
        return Err(ImportError::EmptyGeometry);
    }

    let node_paths = paths
        .iter()
        .filter(|path| {
            matches!(
                prim_type(stage, path).as_deref(),
                Some("Xform" | "Mesh" | "PointInstancer")
            ) && !skipped_prototypes
                .iter()
                .any(|skip| path.as_str() == skip || path.as_str().starts_with(&format!("{skip}/")))
        })
        .cloned()
        .collect::<Vec<_>>();
    let node_path_set = node_paths
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let mut drafts = Vec::new();
    for path in &node_paths {
        let prim = stage
            .prim(path.clone())
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let type_name = prim_type(stage, path).unwrap_or_else(|| "Xform".into());
        let node_key = path.as_str().to_owned();
        let parent_key = nearest_parent(path, &node_path_set);
        let mesh_key = effective_mesh_identity(stage, &prim, path, &mesh_indices)?;
        let mesh = mesh_key
            .as_ref()
            .and_then(|key| mesh_indices.get(key).copied());
        let local = local_transform(&prim)?.unwrap_or(DMat4::IDENTITY);
        let local = if parent_key.is_none() {
            conversion * local
        } else {
            local
        };
        let instance_group = mesh_key
            .as_ref()
            .and_then(|key| instance_groups.get(key))
            .cloned();
        let custom_properties = collect_custom_properties(&prim)?;
        let display_name = prim
            .get_metadata::<String>("displayName")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                path.name()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("USD Prim")
                    .to_owned()
            });
        let object_type = if mesh.is_some() {
            "MESH".to_owned()
        } else {
            type_name
        };
        drafts.push(NodeDraft {
            key: node_key,
            name: display_name.clone(),
            parent_key,
            matrix: local,
            mesh,
            extras: json!({
                "ice3": {
                    "displayName": display_name,
                    "usd": {
                        "objectType": object_type,
                        "meshData": mesh_key,
                        "usdPrimPath": path.as_str(),
                        "instanceGroup": instance_group,
                        "customProperties": custom_properties
                    }
                }
            }),
        });
    }

    for instancer in &point_instancers {
        expand_point_instancer(
            stage,
            instancer,
            &mesh_indices,
            &instance_groups,
            &mut drafts,
        )?;
    }

    drafts.sort_by(|left, right| left.key.cmp(&right.key));
    let rebase = rebase_drafts(&mut drafts, &meshes)?;
    let ids = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| (draft.key.clone(), format!("node-{index:06}")))
        .collect::<BTreeMap<_, _>>();
    let nodes = drafts
        .into_iter()
        .enumerate()
        .map(|(ordinal, draft)| {
            let id = ids.get(&draft.key).cloned().ok_or_else(|| {
                ImportError::InvalidSource("USD node ID mapping is incomplete".into())
            })?;
            let parent = draft
                .parent_key
                .as_ref()
                .and_then(|key| ids.get(key))
                .cloned();
            let mut extras = draft.extras;
            extras["ice3"]["id"] = Value::from(id.clone());
            extras["ice3"]["selectionId"] = Value::from(format!("{asset_id}/usd/node/{ordinal}"));
            Ok(SourceNode {
                id,
                name: draft.name,
                parent,
                matrix: draft
                    .matrix
                    .to_cols_array()
                    .into_iter()
                    .map(|value| finite_f32(value, "matrix"))
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .map_err(|_| ImportError::InvalidSource("USD node matrix is invalid".into()))?,
                mesh: draft.mesh,
                material: draft
                    .mesh
                    .and_then(|mesh| meshes.get(mesh))
                    .and_then(|mesh| mesh.primitives.first())
                    .and_then(|primitive| primitive.material),
                extras,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    let parent_set = nodes
        .iter()
        .filter_map(|node| node.parent.as_deref())
        .collect::<BTreeSet<_>>();
    debug_assert!(
        nodes
            .iter()
            .all(|node| node.parent.is_none()
                || parent_set.contains(node.parent.as_deref().unwrap()))
    );
    let scene = SourceScene {
        asset_id: asset_id.into(),
        source_format: SourceFormat::Usd,
        nodes,
        meshes,
        materials: material_list,
        warnings: warnings.into_iter().collect(),
        asset_extras: json!({
            "coordinateSystem": {
                "handedness": "right",
                "upAxis": "y",
                "linearUnit": "meter"
            },
            "ice3": {
                "usd": {
                    "converter": "native-rust",
                    "sourceFile": root_file.file_name().and_then(|value| value.to_str()),
                    "sourceOrigin": rebase.origin,
                    "originRebased": rebase.flattened,
                    "unitAssumption": "meter",
                    "originalMetersPerUnit": meters_per_unit,
                    "sourceUpAxis": up_axis,
                    "sourceHandedness": handedness,
                    "compositionLayerCount": composition.layers,
                    "compositionDepth": composition.depth
                }
            }
        }),
    };
    validate_scene_limits(&scene, &limits)?;
    Ok(scene)
}

fn import_materials(
    stage: &Stage,
    paths: &[sdf::Path],
    asset_id: &str,
) -> Result<Vec<(String, SourceMaterial)>, ImportError> {
    let material_paths = paths
        .iter()
        .filter(|path| prim_type(stage, path).as_deref() == Some("Material"))
        .cloned()
        .collect::<Vec<_>>();
    material_paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let prim = stage
                .prim(path.clone())
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
            let shader = preview_surface_shader(&prim)?;
            let base_color = match shader
                .as_ref()
                .map(|shader| {
                    shader
                        .attribute("inputs:diffuseColor")
                        .get::<[f32; 3]>()
                        .map_err(|error| ImportError::InvalidSource(error.to_string()))
                })
                .transpose()?
                .flatten()
            {
                Some(value) => finite_f32_value(value)?,
                None => [1.0; 3],
            };
            let opacity = shader
                .as_ref()
                .map(|shader| {
                    shader
                        .attribute("inputs:opacity")
                        .get::<f32>()
                        .map_err(|error| ImportError::InvalidSource(error.to_string()))
                })
                .transpose()?
                .flatten()
                .map(|value| finite_f32(f64::from(value), "material opacity"))
                .transpose()?
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let id = format!("{asset_id}/usd/material/{index}");
            Ok((
                path.as_str().to_owned(),
                SourceMaterial {
                    id: id.clone(),
                    name: path.name().unwrap_or("USD Material").to_owned(),
                    base_color,
                    opacity,
                    extras: json!({
                        "ice3": {
                            "id": id,
                            "usd": {
                                "usdPrimPath": path.as_str()
                            }
                        }
                    }),
                },
            ))
        })
        .collect()
}

fn preview_surface_shader(material: &Prim) -> Result<Option<Prim>, ImportError> {
    let mut pending = material
        .children()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    while let Some(prim) = pending.pop() {
        let shader_id = prim
            .attribute("info:id")
            .get::<Token>()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        if shader_id
            .as_ref()
            .is_some_and(|value| value.as_str() == "UsdPreviewSurface")
        {
            return Ok(Some(prim));
        }
        pending.extend(
            prim.children()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?,
        );
    }
    Ok(None)
}

fn bound_material(
    prim: &Prim,
    material_by_path: &BTreeMap<String, usize>,
) -> Result<Option<usize>, ImportError> {
    let targets = prim
        .relationship("material:binding")
        .forwarded_targets()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    Ok(targets
        .iter()
        .find_map(|target| material_by_path.get(target.as_str()).copied()))
}

fn import_mesh(
    prim: &Prim,
    key: &str,
    index: usize,
    asset_id: &str,
    material: Option<usize>,
) -> Result<(SourceMesh, u64), ImportError> {
    let points = required_vec3(prim, "points")?;
    let counts = prim
        .attribute("faceVertexCounts")
        .get::<Vec<i32>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .ok_or_else(|| ImportError::InvalidSource("USD mesh has no face counts".into()))?;
    let indices = prim
        .attribute("faceVertexIndices")
        .get::<Vec<i32>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .ok_or_else(|| ImportError::InvalidSource("USD mesh has no face indices".into()))?;
    let normals = optional_vec3(prim, "normals")?;
    let normal_interpolation = attribute_interpolation(prim, "normals")?;
    let colors = optional_vec3(prim, "primvars:displayColor")?;
    let color_interpolation = attribute_interpolation(prim, "primvars:displayColor")?;

    let face_counts = counts
        .into_iter()
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| ImportError::InvalidSource("USD face count is negative".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut triangle_corners = Vec::<[usize; 3]>::new();
    let mut corner_count = 0_usize;
    let mut offset = 0_usize;
    for count in &face_counts {
        let count = *count;
        if count < 3 {
            return Err(ImportError::InvalidSource(
                "USD mesh contains a face with fewer than three vertices".into(),
            ));
        }
        corner_count = corner_count
            .checked_add(count)
            .ok_or_else(|| ImportError::LimitExceeded("USD corner count overflowed".into()))?;
        for triangle in 1..count - 1 {
            let corners = [offset, offset + triangle, offset + triangle + 1];
            triangle_corners.push(corners);
        }
        offset = offset
            .checked_add(count)
            .ok_or_else(|| ImportError::LimitExceeded("USD index offset overflowed".into()))?;
    }
    if offset != indices.len() {
        return Err(ImportError::InvalidSource(
            "USD face counts do not match face vertex indices".into(),
        ));
    }

    let mut positions = Vec::with_capacity(triangle_corners.len() * 3);
    let mut output_normals = normals
        .as_ref()
        .map(|_| Vec::with_capacity(positions.capacity()));
    let mut output_colors = colors
        .as_ref()
        .map(|_| Vec::with_capacity(positions.capacity()));
    let mut output_indices = Vec::with_capacity(positions.capacity());
    let mut face_corner_starts = Vec::with_capacity(face_counts.len());
    let mut cursor = 0;
    for count in face_counts {
        face_corner_starts.push(cursor);
        cursor += count;
    }
    for corners in triangle_corners {
        for corner in corners {
            let point_index =
                usize::try_from(*indices.get(corner).ok_or_else(|| {
                    ImportError::InvalidSource("USD face index is missing".into())
                })?)
                .map_err(|_| ImportError::InvalidSource("USD face index is negative".into()))?;
            let point = points.get(point_index).ok_or_else(|| {
                ImportError::InvalidSource("USD face index points outside the vertex array".into())
            })?;
            positions.push(finite_vec3(*point, "position")?);
            if let (Some(values), Some(output)) = (&normals, &mut output_normals) {
                let value = interpolated_vec3(
                    values,
                    &normal_interpolation,
                    point_index,
                    corner,
                    &face_corner_starts,
                    points.len(),
                );
                if let Some(value) = value {
                    output.push(finite_vec3(*value, "normal")?);
                }
            }
            if let (Some(values), Some(output)) = (&colors, &mut output_colors) {
                let value = interpolated_vec3(
                    values,
                    &color_interpolation,
                    point_index,
                    corner,
                    &face_corner_starts,
                    points.len(),
                );
                if let Some(value) = value {
                    output.push(finite_vec3(*value, "color")?);
                }
            }
            output_indices.push(u32::try_from(positions.len() - 1).map_err(|_| {
                ImportError::LimitExceeded("USD primitive vertex count overflowed".into())
            })?);
        }
    }
    if output_normals
        .as_ref()
        .is_some_and(|values| values.len() != positions.len())
    {
        output_normals = None;
    }
    if output_colors
        .as_ref()
        .is_some_and(|values| values.len() != positions.len())
    {
        output_colors = None;
    }
    let triangles = u64::try_from(output_indices.len() / 3)
        .map_err(|_| ImportError::LimitExceeded("USD triangle count overflowed".into()))?;
    let id = format!("{asset_id}/usd/mesh/{index}");
    Ok((
        SourceMesh {
            id: id.clone(),
            primitives: vec![SourcePrimitive {
                positions,
                normals: output_normals,
                colors: output_colors,
                indices: output_indices,
                material,
            }],
            extras: json!({
                "ice3": {
                    "id": id,
                    "usd": {
                        "sourcePrimPath": key
                    }
                }
            }),
        },
        triangles,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interpolation {
    Constant,
    Uniform,
    Vertex,
    FaceVarying,
    Other,
}

fn attribute_interpolation(prim: &Prim, name: &str) -> Result<Interpolation, ImportError> {
    let value = prim
        .attribute(name)
        .get_metadata::<Token>("interpolation")
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .map(|token| match token.as_str() {
            "constant" => Interpolation::Constant,
            "uniform" => Interpolation::Uniform,
            "vertex" => Interpolation::Vertex,
            "faceVarying" => Interpolation::FaceVarying,
            _ => Interpolation::Other,
        })
        .unwrap_or(Interpolation::Vertex);
    Ok(value)
}

fn interpolated_vec3<'a>(
    values: &'a [[f32; 3]],
    interpolation: &Interpolation,
    point: usize,
    corner: usize,
    face_starts: &[usize],
    point_count: usize,
) -> Option<&'a [f32; 3]> {
    let face = face_starts
        .partition_point(|start| *start <= corner)
        .saturating_sub(1);
    let index = match interpolation {
        Interpolation::Constant => 0,
        Interpolation::Vertex => point,
        Interpolation::FaceVarying => corner,
        Interpolation::Uniform => face,
        Interpolation::Other => point,
    };
    if index < values.len() {
        Some(&values[index])
    } else if interpolation == &Interpolation::Vertex && values.len() == point_count {
        values.get(point)
    } else {
        None
    }
}

fn required_vec3(prim: &Prim, name: &str) -> Result<Vec<[f32; 3]>, ImportError> {
    prim.attribute(name)
        .get::<Vec<[f32; 3]>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .filter(|values| !values.is_empty())
        .ok_or_else(|| ImportError::InvalidSource(format!("USD mesh has no {name}")))
}

fn optional_vec3(prim: &Prim, name: &str) -> Result<Option<Vec<[f32; 3]>>, ImportError> {
    prim.attribute(name)
        .get::<Vec<[f32; 3]>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))
}

fn collect_custom_properties(prim: &Prim) -> Result<Map<String, Value>, ImportError> {
    let mut properties = Map::new();
    if let Some(SdfValue::Dictionary(values)) = prim
        .custom_data()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    {
        for (key, value) in values {
            if let Some(value) = metadata_value(&value, 0) {
                properties.insert(key, value);
            }
        }
    }
    for attribute in prim
        .authored_attributes_in_namespace("userProperties")
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    {
        let Some(name) = attribute.name().strip_prefix("userProperties:") else {
            continue;
        };
        if let Some(value) = attribute
            .get::<SdfValue>()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .and_then(|value| metadata_value(&value, 0))
        {
            properties.insert(name.to_owned(), value);
        }
    }
    Ok(properties)
}

fn metadata_value(value: &SdfValue, depth: usize) -> Option<Value> {
    if depth >= MAX_METADATA_DEPTH {
        return None;
    }
    match value {
        SdfValue::Bool(value) => Some(Value::Bool(*value)),
        SdfValue::Uchar(value) => Some(Value::from(*value)),
        SdfValue::Int(value) => Some(Value::from(*value)),
        SdfValue::Uint(value) => Some(Value::from(*value)),
        SdfValue::Int64(value) => Some(Value::from(*value)),
        SdfValue::Uint64(value) => Some(Value::from(*value)),
        SdfValue::Float(value) => finite_json(f64::from(*value)),
        SdfValue::Double(value) => finite_json(*value),
        SdfValue::String(value) => Some(Value::from(value.clone())),
        SdfValue::Token(value) => Some(Value::from(value.as_str())),
        SdfValue::Dictionary(values) => {
            let mut output = Map::new();
            for (key, value) in values {
                if let Some(value) = metadata_value(value, depth + 1) {
                    output.insert(key.clone(), value);
                }
            }
            Some(Value::Object(output))
        }
        SdfValue::BoolVec(values) => Some(Value::from(
            values
                .iter()
                .map(|value| Value::Bool(*value))
                .collect::<Vec<_>>(),
        )),
        SdfValue::IntVec(values) => Some(Value::from(
            values
                .iter()
                .map(|value| Value::from(*value))
                .collect::<Vec<_>>(),
        )),
        SdfValue::Int64Vec(values) => Some(Value::from(
            values
                .iter()
                .map(|value| Value::from(*value))
                .collect::<Vec<_>>(),
        )),
        SdfValue::FloatVec(values) => values
            .iter()
            .map(|value| finite_json(f64::from(*value)))
            .collect::<Option<Vec<_>>>()
            .map(Value::from),
        SdfValue::DoubleVec(values) => values
            .iter()
            .map(|value| finite_json(*value))
            .collect::<Option<Vec<_>>>()
            .map(Value::from),
        SdfValue::StringVec(values) => Some(Value::from(
            values
                .iter()
                .map(|value| Value::from(value.clone()))
                .collect::<Vec<_>>(),
        )),
        SdfValue::TokenVec(values) => Some(Value::from(
            values
                .iter()
                .map(|value| Value::from(value.as_str()))
                .collect::<Vec<_>>(),
        )),
        SdfValue::Vec3f(value) => {
            finite_vector_json([f64::from(value.x), f64::from(value.y), f64::from(value.z)])
        }
        SdfValue::Vec3d(value) => finite_vector_json([value.x, value.y, value.z]),
        _ => None,
    }
    .filter(|_| !value_is_oversized_array(value))
}

fn finite_vector_json(values: [f64; 3]) -> Option<Value> {
    let values = values
        .into_iter()
        .map(finite_json)
        .collect::<Option<Vec<_>>>()?;
    Some(Value::Array(values))
}

fn value_is_oversized_array(value: &SdfValue) -> bool {
    value
        .array_len()
        .is_some_and(|length| length > MAX_METADATA_ARRAY_LENGTH)
}

fn local_transform(prim: &Prim) -> Result<Option<DMat4>, ImportError> {
    let order = prim
        .attribute("xformOpOrder")
        .get::<Vec<String>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    let mut matrix = DMat4::IDENTITY;
    if let Some(order) = order {
        for operation in order {
            if let Some(operation) = transform_operation(prim, &operation)? {
                matrix *= operation;
            }
        }
        return Ok(Some(matrix));
    }
    for operation in [
        "xformOp:transform",
        "xformOp:translate",
        "xformOp:rotateX",
        "xformOp:rotateY",
        "xformOp:rotateZ",
        "xformOp:orient",
        "xformOp:scale",
    ] {
        if let Some(operation) = transform_operation(prim, operation)? {
            matrix *= operation;
        }
    }
    Ok((matrix != DMat4::IDENTITY).then_some(matrix))
}

fn transform_operation(prim: &Prim, name: &str) -> Result<Option<DMat4>, ImportError> {
    let attribute = prim.attribute(name);
    match name {
        "xformOp:transform" => {
            let Some(matrix) = attribute
                .get::<Matrix4d>()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            else {
                return Ok(None);
            };
            let mut transposed = [0.0_f64; 16];
            for row in 0..4 {
                for column in 0..4 {
                    transposed[column * 4 + row] = matrix.0[row * 4 + column];
                }
            }
            if !transposed.iter().all(|value| value.is_finite()) {
                return Err(ImportError::InvalidSource(
                    "USD matrix contains a non-finite value".into(),
                ));
            }
            Ok(Some(DMat4::from_cols_array(&transposed)))
        }
        "xformOp:translate" => Ok(attribute
            .get::<[f64; 3]>()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .map(|value| DMat4::from_translation(DVec3::new(value[0], value[1], value[2])))),
        "xformOp:scale" => Ok(attribute
            .get::<[f32; 3]>()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .map(|value| {
                DMat4::from_scale(DVec3::new(
                    f64::from(value[0]),
                    f64::from(value[1]),
                    f64::from(value[2]),
                ))
            })),
        "xformOp:orient" => Ok(attribute
            .get::<openusd::gf::Quatf>()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .map(|value| {
                DMat4::from_quat(DQuat::from_xyzw(
                    f64::from(value.x),
                    f64::from(value.y),
                    f64::from(value.z),
                    f64::from(value.w),
                ))
            })),
        "xformOp:rotateX" => rotation_operation(attribute, DVec3::X),
        "xformOp:rotateY" => rotation_operation(attribute, DVec3::Y),
        "xformOp:rotateZ" => rotation_operation(attribute, DVec3::Z),
        _ => Ok(None),
    }
}

fn rotation_operation(
    attribute: openusd::usd::Attribute,
    axis: DVec3,
) -> Result<Option<DMat4>, ImportError> {
    Ok(attribute
        .get::<f32>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .map(|degrees| DMat4::from_axis_angle(axis, f64::from(degrees).to_radians())))
}

fn coordinate_conversion(
    meters_per_unit: f64,
    up_axis: &str,
    handedness: &str,
) -> Result<DMat4, ImportError> {
    if !meters_per_unit.is_finite() || meters_per_unit <= 0.0 {
        return Err(ImportError::InvalidSource(
            "USD metersPerUnit must be finite and positive".into(),
        ));
    }
    let scale = DMat4::from_scale(DVec3::splat(meters_per_unit));
    let up = match up_axis.to_ascii_uppercase().as_str() {
        "Y" => DMat4::IDENTITY,
        "Z" => DMat4::from_rotation_x(-std::f64::consts::FRAC_PI_2),
        "X" => DMat4::from_rotation_z(std::f64::consts::FRAC_PI_2),
        _ => {
            return Err(ImportError::InvalidSource(format!(
                "USD upAxis {up_axis:?} is unsupported"
            )));
        }
    };
    let handed = match handedness.to_ascii_lowercase().as_str() {
        "right" | "righthanded" => DMat4::IDENTITY,
        "left" | "lefthanded" => DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)),
        _ => {
            return Err(ImportError::InvalidSource(format!(
                "USD handedness {handedness:?} is unsupported"
            )));
        }
    };
    Ok(scale * up * handed)
}

fn mesh_identity(prim: &Prim, path: &sdf::Path) -> Result<String, ImportError> {
    if let Some(prototype) = prim
        .prototype()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    {
        return Ok(prototype.to_string());
    }
    if let Some(prototype) = prim
        .prim_in_prototype()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    {
        return Ok(prototype.path().to_string());
    }
    Ok(path.to_string())
}

fn effective_mesh_identity(
    stage: &Stage,
    prim: &Prim,
    path: &sdf::Path,
    mesh_indices: &BTreeMap<String, usize>,
) -> Result<Option<String>, ImportError> {
    if prim_type(stage, path).as_deref() == Some("Mesh") {
        return Ok(Some(mesh_identity(prim, path)?));
    }
    let prototype = prim
        .prototype()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    if let Some(prototype) = prototype
        && prim_type(stage, &prototype).as_deref() == Some("Mesh")
    {
        return Ok(Some(prototype.to_string()));
    }
    if prim
        .attribute("points")
        .get::<Vec<[f32; 3]>>()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        .is_some()
    {
        let source = prim
            .prim_stack()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .into_iter()
            .map(|site| site.path.to_string())
            .find(|candidate| mesh_indices.contains_key(candidate));
        if source.is_some() {
            return Ok(source);
        }
    }
    Ok(None)
}

fn collect_point_instancers(
    stage: &Stage,
    paths: &[sdf::Path],
) -> Result<Vec<PointInstancer>, ImportError> {
    let mut output = Vec::new();
    for path in paths {
        if prim_type(stage, path).as_deref() != Some("PointInstancer") {
            continue;
        }
        let prim = stage
            .prim(path.clone())
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let prototypes = prim
            .relationship("prototypes")
            .forwarded_targets()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
            .into_iter()
            .map(|path| path.to_string())
            .collect::<Vec<_>>();
        if prototypes.is_empty() {
            continue;
        }
        output.push(PointInstancer {
            path: path.as_str().to_owned(),
            prototypes,
            positions: prim
                .attribute("positions")
                .get::<Vec<[f32; 3]>>()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?
                .unwrap_or_default(),
            orientations: prim
                .attribute("orientations")
                .get::<Vec<openusd::gf::Quatf>>()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?
                .unwrap_or_default(),
            scales: prim
                .attribute("scales")
                .get::<Vec<[f32; 3]>>()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?
                .unwrap_or_default(),
            proto_indices: prim
                .attribute("protoIndices")
                .get::<Vec<i32>>()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?
                .unwrap_or_default(),
        });
    }
    Ok(output)
}

struct PointInstancer {
    path: String,
    prototypes: Vec<String>,
    positions: Vec<[f32; 3]>,
    orientations: Vec<openusd::gf::Quatf>,
    scales: Vec<[f32; 3]>,
    proto_indices: Vec<i32>,
}

fn expand_point_instancer(
    stage: &Stage,
    instancer: &PointInstancer,
    mesh_indices: &BTreeMap<String, usize>,
    instance_groups: &BTreeMap<String, String>,
    drafts: &mut Vec<NodeDraft>,
) -> Result<(), ImportError> {
    if instancer.positions.is_empty() {
        return Ok(());
    }
    let prototypes = instancer
        .prototypes
        .iter()
        .map(|prototype| {
            let mesh_path = first_mesh_at_or_below(stage, prototype)?.ok_or_else(|| {
                ImportError::InvalidSource(
                    "USD PointInstancer prototype has no supported mesh".into(),
                )
            })?;
            let prim = stage
                .prim(mesh_path.clone())
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
            let mesh_key = mesh_identity(&prim, &mesh_path)?;
            let mesh_index = mesh_indices.get(&mesh_key).copied().ok_or_else(|| {
                ImportError::InvalidSource(
                    "USD PointInstancer prototype mesh is missing from the mesh index".into(),
                )
            })?;
            Ok((prototype.clone(), mesh_key, mesh_index))
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    if prototypes.is_empty() {
        return Err(ImportError::InvalidSource(
            "USD PointInstancer has no supported prototype mesh".into(),
        ));
    }
    for (index, position) in instancer.positions.iter().enumerate() {
        let proto_index = instancer.proto_indices.get(index).copied().unwrap_or(0);
        let selected = usize::try_from(proto_index)
            .ok()
            .and_then(|value| prototypes.get(value).map(|prototype| (value, prototype)));
        let (prototype_index, (prototype_path, mesh_key, mesh_index)) =
            selected.ok_or_else(|| {
                ImportError::InvalidSource(
                    "USD PointInstancer protoIndices points outside prototypes".into(),
                )
            })?;
        let group = instance_groups.get(mesh_key).cloned();
        let orientation = instancer
            .orientations
            .get(index)
            .copied()
            .unwrap_or(openusd::gf::Quatf::IDENTITY);
        let scale = instancer.scales.get(index).copied().unwrap_or([1.0; 3]);
        let rotation = DQuat::from_xyzw(
            f64::from(orientation.x),
            f64::from(orientation.y),
            f64::from(orientation.z),
            f64::from(orientation.w),
        );
        let matrix = DMat4::from_translation(DVec3::new(
            f64::from(position[0]),
            f64::from(position[1]),
            f64::from(position[2]),
        )) * DMat4::from_quat(rotation)
            * DMat4::from_scale(DVec3::new(
                f64::from(scale[0]),
                f64::from(scale[1]),
                f64::from(scale[2]),
            ));
        let key = format!("{}/__instances/{index:06}", instancer.path);
        drafts.push(NodeDraft {
            key,
            name: format!(
                "{} Instance {index}",
                prototype_path.rsplit('/').next().unwrap_or("USD Prototype")
            ),
            parent_key: Some(instancer.path.clone()),
            matrix,
            mesh: Some(*mesh_index),
            extras: json!({
                "ice3": {
                    "displayName": format!("Point Instance {index}"),
                    "usd": {
                        "objectType": "MESH",
                        "meshData": mesh_key,
                        "usdPrimPath": format!("{}/instance/{index}", instancer.path),
                        "instanceGroup": group,
                        "prototypeIndex": prototype_index
                    }
                }
            }),
        });
    }
    Ok(())
}

fn first_mesh_at_or_below(stage: &Stage, path: &str) -> Result<Option<sdf::Path>, ImportError> {
    let prim = stage
        .prim(path)
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    if prim_type(stage, prim.path()).as_deref() == Some("Mesh") {
        return Ok(Some(prim.path().clone()));
    }
    let mut pending = prim
        .children()
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
    while let Some(prim) = pending.pop() {
        if prim_type(stage, prim.path()).as_deref() == Some("Mesh") {
            return Ok(Some(prim.path().clone()));
        }
        pending.extend(
            prim.children()
                .map_err(|error| ImportError::InvalidSource(error.to_string()))?,
        );
    }
    Ok(None)
}

struct NodeDraft {
    key: String,
    name: String,
    parent_key: Option<String>,
    matrix: DMat4,
    mesh: Option<usize>,
    extras: Value,
}

fn nearest_parent(path: &sdf::Path, node_paths: &BTreeSet<String>) -> Option<String> {
    let mut parent = path.parent();
    while let Some(candidate) = parent {
        if candidate.is_abs_root() {
            return None;
        }
        if node_paths.contains(candidate.as_str()) {
            return Some(candidate.to_string());
        }
        parent = candidate.parent();
    }
    None
}

fn add_resource_warnings(
    stage: &Stage,
    paths: &[sdf::Path],
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<(), ImportError> {
    let mut categories = BTreeSet::new();
    for path in paths {
        let prim = stage
            .prim(path.clone())
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        let Some(type_name) = prim
            .type_name()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        else {
            continue;
        };
        let type_name = type_name.as_str();
        if matches!(
            type_name,
            "Camera"
                | "Skeleton"
                | "SkelRoot"
                | "Volume"
                | "NurbsCurves"
                | "NurbsPatch"
                | "BasisCurves"
        ) || type_name.contains("Light")
        {
            categories.insert(type_name.to_owned());
        }
        for attribute in prim
            .authored_attributes()
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        {
            if attribute.get::<sdf::AssetPath>().ok().flatten().is_some()
                || attribute
                    .get::<Vec<sdf::AssetPath>>()
                    .ok()
                    .flatten()
                    .is_some_and(|values| !values.is_empty())
            {
                categories.insert("Texture".into());
            }
        }
    }
    if !categories.is_empty() {
        let summary = categories.into_iter().collect::<Vec<_>>().join(", ");
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_USD_UNSUPPORTED_RESOURCE",
            &format!("USD resources are not imported: {summary}"),
        ));
    }
    Ok(())
}

fn warning(code: &str, message: &str) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message: message.into(),
        path: None,
    }
}

fn prim_type(stage: &Stage, path: &sdf::Path) -> Option<String> {
    stage
        .prim(path.clone())
        .ok()?
        .type_name()
        .ok()?
        .map(|value| value.as_str().to_owned())
}

fn stage_number(stage: &Stage, field: &str) -> Option<f64> {
    match stage.stage_metadata(field).ok()?? {
        SdfValue::Float(value) => Some(f64::from(value)),
        SdfValue::Double(value) => Some(value),
        SdfValue::Int(value) => Some(f64::from(value)),
        SdfValue::Int64(value) => Some(value as f64),
        SdfValue::Half(value) => Some(f64::from(f32::from(value))),
        _ => None,
    }
}

fn stage_token(stage: &Stage, field: &str) -> Option<String> {
    match stage.stage_metadata(field).ok()?? {
        SdfValue::Token(value) => Some(value.as_str().to_owned()),
        SdfValue::String(value) => Some(value),
        _ => None,
    }
}

fn rebase_drafts(
    drafts: &mut [NodeDraft],
    meshes: &[SourceMesh],
) -> Result<RebaseResult, ImportError> {
    let draft_indices = drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| (draft.key.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut world_matrices = Vec::with_capacity(drafts.len());
    let mut bounds = Bounds::empty();
    for draft in drafts.iter() {
        let world = draft
            .parent_key
            .as_deref()
            .and_then(|parent| draft_indices.get(parent))
            .map_or(draft.matrix, |parent| {
                world_matrices[*parent] * draft.matrix
            });
        world_matrices.push(world);
        let Some(mesh) = draft.mesh.and_then(|index| meshes.get(index)) else {
            continue;
        };
        for primitive in &mesh.primitives {
            for point in &primitive.positions {
                let world_point = world.transform_point3(DVec3::new(
                    f64::from(point[0]),
                    f64::from(point[1]),
                    f64::from(point[2]),
                ));
                bounds.include(world_point)?;
            }
        }
    }
    let bounds = bounds.finish().ok_or(ImportError::EmptyGeometry)?;
    let origin = [bounds.min[0], 0.0, bounds.min[2]];
    let flattened = origin[0].abs() > 10_000.0 || origin[2].abs() > 10_000.0;
    let desired_worlds = world_matrices
        .iter()
        .map(|matrix| {
            let mut matrix = *matrix;
            matrix.w_axis.x -= origin[0];
            matrix.w_axis.z -= origin[2];
            matrix
        })
        .collect::<Vec<_>>();
    let actual_worlds = if flattened {
        drafts
            .iter()
            .zip(&desired_worlds)
            .map(|(draft, desired)| {
                if draft.mesh.is_some() {
                    *desired
                } else {
                    DMat4::IDENTITY
                }
            })
            .collect::<Vec<_>>()
    } else {
        desired_worlds
    };
    for index in 0..drafts.len() {
        let local = if let Some(parent) = drafts[index]
            .parent_key
            .as_deref()
            .and_then(|parent| draft_indices.get(parent))
        {
            let inverse = actual_worlds[*parent].inverse();
            if !inverse.is_finite() {
                return Err(ImportError::InvalidSource(
                    "USD parent transform cannot be inverted".into(),
                ));
            }
            inverse * actual_worlds[index]
        } else {
            actual_worlds[index]
        };
        drafts[index].matrix = local;
    }
    Ok(RebaseResult { origin, flattened })
}

struct RebaseResult {
    origin: [f64; 3],
    flattened: bool,
}

struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    fn include(&mut self, point: DVec3) -> Result<(), ImportError> {
        if !point.is_finite() {
            return Err(ImportError::InvalidSource(
                "USD geometry contains a non-finite position".into(),
            ));
        }
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(point[axis]);
            self.max[axis] = self.max[axis].max(point[axis]);
        }
        Ok(())
    }

    fn finish(&self) -> Option<Self> {
        if self.min.iter().all(|value| value.is_finite())
            && self.max.iter().all(|value| value.is_finite())
        {
            Some(Self {
                min: self.min,
                max: self.max,
            })
        } else {
            None
        }
    }
}

fn finite_vec3(value: [f32; 3], field: &str) -> Result<[f32; 3], ImportError> {
    value
        .into_iter()
        .map(|value| finite_f32(f64::from(value), field))
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| ImportError::InvalidSource(format!("USD {field} is invalid")))
}

fn finite_f32_value(value: [f32; 3]) -> Result<[f32; 3], ImportError> {
    finite_vec3(value, "material color")
}

fn finite_f32(value: f64, field: &str) -> Result<f32, ImportError> {
    let value = value as f32;
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| ImportError::InvalidSource(format!("USD {field} is not finite")))
}

fn finite_json(value: f64) -> Option<Value> {
    value.is_finite().then(|| Value::from(value))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_and_uri_asset_paths() {
        assert!(validate_authored_asset_path("../outside.usda").is_err());
        assert!(validate_authored_asset_path("https://example.test/a.usda").is_err());
        assert!(validate_authored_asset_path("/absolute/a.usda").is_err());
        assert!(validate_authored_asset_path("./inside/a.usda").is_ok());
    }

    #[test]
    fn truncates_unsupported_metadata_values() {
        assert!(metadata_value(&SdfValue::Bool(true), 0).is_some());
        assert!(
            metadata_value(
                &SdfValue::IntVec((0..=MAX_METADATA_ARRAY_LENGTH as i32).collect()),
                0
            )
            .is_none()
        );
    }
}
