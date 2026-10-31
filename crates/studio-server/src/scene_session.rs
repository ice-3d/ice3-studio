use std::collections::{BTreeMap, BTreeSet};

use studio_core::diff::DocumentDiff;
use studio_core::document::{Document, NodeContent};
use studio_core::geometry::{EvaluatedMesh, TriangleMesh};
use studio_core::ids::{ComponentId, MaterialId, NodeId};
use studio_core::large_scene::cache::{GeometryEvaluator, GeometryStore};
use studio_core::large_scene::component::PartContent;
use studio_core::large_scene::evaluate::{
    EvaluatedScene, EvaluatedSceneNode, LargeSceneError, evaluate_large_scene_selected,
    evaluate_large_scene_with_cache_and_evaluator,
};
use studio_core::math::Transform;
use studio_core::scene_contract::{
    SceneGeometryChange, SceneInstanceChange, ScenePatch, changed_position_ranges,
    scene_content_dirty,
};
use studio_gpu::ResidencyKey;

const CPU_KERNEL_VERSION: &str = "geometry-kernel-v1";

pub struct SceneSourceIndex {
    revision: u64,
    nodes: BTreeMap<NodeId, SourceNodeSnapshot>,
    component_instances: BTreeMap<ComponentId, BTreeSet<NodeId>>,
    component_parents: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    material_nodes: BTreeMap<MaterialId, BTreeSet<NodeId>>,
    expanded_by_source: BTreeMap<NodeId, Vec<String>>,
}

struct SourceNodeSnapshot {
    content_digest: String,
}

impl SceneSourceIndex {
    pub fn capture(
        document: &Document,
        scene: &EvaluatedScene,
        _evaluator: Option<&dyn GeometryEvaluator>,
    ) -> Result<Self, LargeSceneError> {
        let mut nodes = BTreeMap::new();
        let mut component_instances = BTreeMap::<ComponentId, BTreeSet<NodeId>>::new();
        let mut material_nodes = BTreeMap::<MaterialId, BTreeSet<NodeId>>::new();
        for node in document.scene.nodes() {
            let key = ResidencyKey::for_content(&node.content, "", CPU_KERNEL_VERSION)?;
            nodes.insert(
                node.id.clone(),
                SourceNodeSnapshot {
                    content_digest: key.attribute,
                },
            );
            match &node.content {
                NodeContent::ComponentInstance(instance) => {
                    component_instances
                        .entry(instance.definition_id.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
                NodeContent::InstanceSet(set) => {
                    component_instances
                        .entry(set.definition_id.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
                _ => {}
            }
            if let Some(material_id) = &node.material_id {
                material_nodes
                    .entry(material_id.clone())
                    .or_default()
                    .insert(node.id.clone());
            }
        }

        let mut component_parents = BTreeMap::<ComponentId, BTreeSet<ComponentId>>::new();
        for definition in &document.components {
            for part in &definition.parts {
                if let PartContent::ComponentReference { definition_id, .. } = &part.content {
                    component_parents
                        .entry(definition_id.clone())
                        .or_default()
                        .insert(definition.id.clone());
                }
            }
        }

        let source_ids = document
            .scene
            .nodes()
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut expanded_by_source = BTreeMap::<NodeId, Vec<String>>::new();
        for node in &scene.nodes {
            let source = node.owner_id.strip_suffix("/").unwrap_or(&node.owner_id);
            let matched = source_ids
                .iter()
                .filter(|source_id| {
                    node.id.as_str() == **source_id
                        || node
                            .id
                            .strip_prefix(*source_id)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                        || node.owner_id == **source_id
                })
                .max_by_key(|source_id| source_id.len())
                .copied();
            if let Some(source_id) = matched {
                expanded_by_source
                    .entry(NodeId::from(source_id))
                    .or_default()
                    .push(node.id.clone());
            } else if source_ids.contains(source) {
                expanded_by_source
                    .entry(NodeId::from(source))
                    .or_default()
                    .push(node.id.clone());
            }
        }

        Ok(Self {
            revision: document.revision.as_u64(),
            nodes,
            component_instances,
            component_parents,
            material_nodes,
            expanded_by_source,
        })
    }

    pub fn affected_source_nodes(&self, diff: &DocumentDiff) -> Option<BTreeSet<NodeId>> {
        if !diff.updated_relation_ids.is_empty()
            || !diff.removed_relation_ids.is_empty()
            || !diff.added_relation_ids.is_empty()
            || !diff.updated_constraint_ids.is_empty()
            || !diff.removed_constraint_ids.is_empty()
            || !diff.added_constraint_ids.is_empty()
        {
            return None;
        }

        let mut selected = BTreeSet::new();
        for id in diff
            .added_node_ids
            .iter()
            .chain(&diff.updated_node_ids)
            .chain(&diff.removed_node_ids)
        {
            selected.insert(id.clone());
        }
        for id in diff
            .added_material_ids
            .iter()
            .chain(&diff.updated_material_ids)
            .chain(&diff.removed_material_ids)
        {
            if let Some(nodes) = self.material_nodes.get(id) {
                selected.extend(nodes.iter().cloned());
            }
        }

        let mut changed_components = diff
            .added_component_ids
            .iter()
            .chain(&diff.updated_component_ids)
            .chain(&diff.removed_component_ids)
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut cursor = changed_components.clone();
        while let Some(component) = cursor.pop_first() {
            if let Some(parents) = self.component_parents.get(&component) {
                for parent in parents {
                    if changed_components.insert(parent.clone()) {
                        cursor.insert(parent.clone());
                    }
                }
            }
        }
        for component in changed_components {
            if let Some(nodes) = self.component_instances.get(&component) {
                selected.extend(nodes.iter().cloned());
            }
        }
        Some(selected)
    }

    pub fn expanded_node_ids(&self, source: &NodeId) -> impl Iterator<Item = &str> {
        self.expanded_by_source
            .get(source)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }
}

pub fn build_full_scene_patch(
    document: &Document,
    scene: &EvaluatedScene,
    _evaluator: Option<&dyn GeometryEvaluator>,
) -> Result<ScenePatch, LargeSceneError> {
    let mut geometry = BTreeMap::new();
    let mut instances = Vec::new();
    for node in &scene.nodes {
        append_geometry(scene, scene, node, None, &mut geometry)?;
        instances.push(instance_change(node));
    }
    Ok(ScenePatch {
        base_revision: 0,
        target_revision: document.revision.as_u64(),
        full_rebuild: true,
        geometry: geometry.into_values().collect(),
        instances,
        removed_geometry_ids: Vec::new(),
        removed_node_ids: Vec::new(),
    })
}

pub fn build_scene_patch<S: GeometryStore>(
    document: &Document,
    diff: &DocumentDiff,
    previous_scene: &EvaluatedScene,
    index: &SceneSourceIndex,
    store: &mut S,
    evaluator: Option<&dyn GeometryEvaluator>,
) -> Result<ScenePatch, LargeSceneError> {
    let Some(selected) = index.affected_source_nodes(diff) else {
        let scene = evaluate_large_scene_with_cache_and_evaluator(document, store, evaluator)?;
        return build_full_scene_patch(document, &scene, evaluator);
    };

    let scene = evaluate_large_scene_selected(document, store, evaluator, &selected)?;
    let scene_by_id = scene
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let previous_by_id = previous_scene
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut geometry = BTreeMap::new();
    let mut instances = Vec::new();
    let force_geometry = !diff.added_component_ids.is_empty()
        || !diff.updated_component_ids.is_empty()
        || !diff.removed_component_ids.is_empty();

    for source in &selected {
        let prior = index.nodes.get(source);
        let source_node = document
            .scene
            .nodes()
            .iter()
            .find(|node| node.id == *source);
        let content_changed = source_node.is_some_and(|node| {
            let key = ResidencyKey::for_content(&node.content, "", CPU_KERNEL_VERSION)
                .expect("valid scene content");
            prior.is_none_or(|prior| prior.content_digest != key.attribute)
        });
        for expanded in index.expanded_node_ids(source) {
            if let Some(node) = scene_by_id.get(expanded) {
                if force_geometry || content_changed {
                    append_geometry(
                        &scene,
                        previous_scene,
                        node,
                        previous_by_id.get(expanded).copied(),
                        &mut geometry,
                    )?;
                }
                instances.push(instance_change(node));
            }
        }
    }

    let mut removed_node_ids = Vec::new();
    for source in &diff.removed_node_ids {
        removed_node_ids.extend(index.expanded_node_ids(source).map(str::to_owned));
    }

    Ok(ScenePatch {
        base_revision: index.revision,
        target_revision: document.revision.as_u64(),
        full_rebuild: false,
        geometry: geometry.into_values().collect(),
        instances,
        removed_geometry_ids: Vec::new(),
        removed_node_ids,
    })
}

fn append_geometry(
    current_scene: &EvaluatedScene,
    previous_scene: &EvaluatedScene,
    node: &EvaluatedSceneNode,
    previous: Option<&EvaluatedSceneNode>,
    output: &mut BTreeMap<String, SceneGeometryChange>,
) -> Result<(), LargeSceneError> {
    let Some(mesh_key) = &node.mesh_key else {
        return Ok(());
    };
    let Some(mesh) = current_scene.meshes.get(mesh_key) else {
        return Ok(());
    };
    let content = mesh_content(mesh);
    let key = ResidencyKey::for_content(&content, "", CPU_KERNEL_VERSION)?;
    let geometry_id = node
        .resident_geometry_id
        .as_ref()
        .map(|id| id.0.clone())
        .unwrap_or_else(|| mesh_key.clone());
    let previous_geometry_id = previous.and_then(|node| {
        node.resident_geometry_id
            .as_ref()
            .map(|id| id.0.clone())
            .or_else(|| node.mesh_key.clone())
    });
    let previous_content = previous.and_then(|node| {
        node.mesh_key
            .as_ref()
            .and_then(|key| previous_scene.meshes.get(key))
            .map(mesh_content)
    });
    let dirty = previous_content.as_ref().map_or_else(
        || {
            scene_content_dirty(
                &content,
                &NodeContent::CustomMesh(TriangleMesh {
                    positions: Vec::new(),
                    indices: Vec::new(),
                }),
            )
        },
        |before| scene_content_dirty(before, &content),
    );
    let position_ranges = previous_content
        .as_ref()
        .and_then(|before| match (&content, before) {
            (NodeContent::CustomMesh(after), NodeContent::CustomMesh(before)) => {
                changed_position_ranges(before, after)
            }
            _ => None,
        })
        .unwrap_or_default();
    let digests = key.digest_set("", CPU_KERNEL_VERSION);
    output.insert(
        geometry_id.clone(),
        SceneGeometryChange {
            geometry_id,
            previous_geometry_id,
            content,
            digests,
            dirty,
            position_ranges,
        },
    );
    Ok(())
}

fn mesh_content(mesh: &EvaluatedMesh) -> NodeContent {
    NodeContent::CustomMesh(TriangleMesh {
        positions: mesh.vertices.iter().map(|vertex| vertex.position).collect(),
        indices: mesh.indices.clone(),
    })
}

fn instance_change(node: &EvaluatedSceneNode) -> SceneInstanceChange {
    SceneInstanceChange {
        node_id: node.id.clone(),
        geometry_id: node
            .resident_geometry_id
            .as_ref()
            .map(|id| id.0.clone())
            .or_else(|| node.mesh_key.clone()),
        matrix: transform_matrix(node.transform),
        material_id: node.material_id.as_ref().map(ToString::to_string),
        visible: node.visible,
    }
}

fn transform_matrix(transform: Transform) -> [f32; 16] {
    let [x, y, z, w] = transform.rotation;
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;
    let [sx, sy, sz] = [transform.scale.x, transform.scale.y, transform.scale.z];
    [
        (1.0 - (yy + zz)) * sx,
        (xy + wz) * sx,
        (xz - wy) * sx,
        0.0,
        (xy - wz) * sy,
        (1.0 - (xx + zz)) * sy,
        (yz + wx) * sy,
        0.0,
        (xz + wy) * sz,
        (yz - wx) * sz,
        (1.0 - (xx + yy)) * sz,
        0.0,
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
        1.0,
    ]
}
