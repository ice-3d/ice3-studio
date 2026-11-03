//! AP242 tessellated STEP export (CAx-IF "3D Tessellated Geometry" practice).
//!
//! Phase 2:
//! * explicit SI-millimetre unit context (`GLOBAL_UNIT_ASSIGNED_CONTEXT`);
//! * per-material surface colours (sRGB → linear) and transparency through the
//!   standard `STYLED_ITEM` → `PRESENTATION_STYLE_ASSIGNMENT` →
//!   `SURFACE_STYLE_USAGE` → `SURFACE_SIDE_STYLE` chain;
//! * instancing: one `SHAPE_REPRESENTATION` per `(mesh, material)` variant, and
//!   each rigid node placed by a `MAPPED_ITEM` (`REPRESENTATION_MAP` with an
//!   identity origin). Non-rigid nodes (scaled / skewed) are baked into their
//!   own face set inside the root representation.
//!
//! Coordinates are baked/emitted in millimetres. The reader in `oxideav-step`
//! discovers a bare `SHAPE_REPRESENTATION` tree as a part hierarchy, so no
//! PRODUCT / assembly `NEXT_ASSEMBLY_USAGE_OCCURRENCE` tree is required.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use glam::{Mat4, Quat, Vec3};

use crate::document::{Document, Material};
use crate::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode};

/// Millimetres per metre: Studio evaluates and stores geometry in metres.
pub const MILLIMETRES_PER_METRE: f32 = 1000.0;

#[derive(Debug, thiserror::Error)]
pub enum StepExportError {
    #[error("STEP export produced no visible geometry")]
    Empty,
}

/// One baked mesh: world-space positions (already in millimetres) plus
/// zero-based triangles.
#[derive(Debug, Clone, PartialEq)]
pub struct StepMeshBucket {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Phase 1 encoder: a single flat part with one face set per bucket.
pub fn encode_step(buckets: &[StepMeshBucket], part_name: &str) -> String {
    let mut writer = StepWriter::new(part_name);
    writer.header();
    let mut items = Vec::new();
    for bucket in buckets {
        if bucket.positions.is_empty() || bucket.triangles.is_empty() {
            continue;
        }
        let coords = writer.coordinates_list(&bucket.name, &bucket.positions);
        let set = writer.triangulated_surface_set(&bucket.name, coords, bucket.positions.len(), &bucket.triangles);
        items.push(set);
    }
    writer.shape_representation(part_name, &items, None);
    writer.finish()
}

/// Flatten an evaluated scene into world-space mesh buckets (millimetres).
pub fn buckets_from_scene(scene: &EvaluatedScene) -> Vec<StepMeshBucket> {
    let world = world_matrices(scene);
    let mut buckets = Vec::new();
    for node in &scene.nodes {
        if !node.visible {
            continue;
        }
        let (Some(mesh_key), Some(matrix)) = (&node.mesh_key, world.get(&node.id)) else {
            continue;
        };
        let Some(mesh) = scene.meshes.get(mesh_key) else {
            continue;
        };
        if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
            continue;
        }
        let mut positions = Vec::with_capacity(mesh.vertices.len());
        for vertex in &mesh.vertices {
            let point = matrix.transform_point3(Vec3::new(
                vertex.position[0],
                vertex.position[1],
                vertex.position[2],
            )) * MILLIMETRES_PER_METRE;
            positions.push([point.x, point.y, point.z]);
        }
        let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
        for chunk in mesh.indices.chunks_exact(3) {
            triangles.push([chunk[0], chunk[1], chunk[2]]);
        }
        buckets.push(StepMeshBucket {
            name: node.name.clone(),
            positions,
            triangles,
        });
    }
    buckets
}

/// Phase 1 wrapper: flat, world-baked, no colours/units.
pub fn export_step_from_scene(scene: &EvaluatedScene) -> Result<String, StepExportError> {
    let buckets = buckets_from_scene(scene);
    if buckets.is_empty() {
        return Err(StepExportError::Empty);
    }
    Ok(encode_step(&buckets, "ice3-scene"))
}

/// Phase 2 encoder: units, colours/transparency and instancing.
pub fn export_step(document: &Document, scene: &EvaluatedScene) -> Result<String, StepExportError> {
    let materials: BTreeMap<&str, &Material> = document
        .materials
        .iter()
        .map(|material| (material.id.as_str(), material))
        .collect();
    let world = world_matrices(scene);

    // Nodes grouped by (mesh, material) so shared geometry is written once.
    let mut variant_instances: BTreeMap<(String, Option<String>), Vec<Mat4>> =
        BTreeMap::new();
    let mut baked: Vec<(String, Vec<[f32; 3]>, Vec<[u32; 3]>, Option<String>)> = Vec::new();

    for node in &scene.nodes {
        if !node.visible {
            continue;
        }
        let (Some(mesh_key), Some(matrix)) = (&node.mesh_key, world.get(&node.id)) else {
            continue;
        };
        let Some(mesh) = scene.meshes.get(mesh_key) else {
            continue;
        };
        if mesh.vertices.is_empty() || mesh.indices.len() < 3 {
            continue;
        }
        let material_id = node.material_id.as_ref().map(|id| id.as_str().to_string());
        let rigid = is_rigid(matrix);
        if rigid {
            variant_instances
                .entry((mesh_key.clone(), material_id))
                .or_default()
                .push(*matrix);
        } else {
            // Bake scaled/skewed transforms into a dedicated face set.
            let mut positions = Vec::with_capacity(mesh.vertices.len());
            for vertex in &mesh.vertices {
                let point = matrix.transform_point3(Vec3::new(
                    vertex.position[0],
                    vertex.position[1],
                    vertex.position[2],
                )) * MILLIMETRES_PER_METRE;
                positions.push([point.x, point.y, point.z]);
            }
            let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
            for chunk in mesh.indices.chunks_exact(3) {
                triangles.push([chunk[0], chunk[1], chunk[2]]);
            }
            baked.push((node.name.clone(), positions, triangles, material_id));
        }
    }

    if variant_instances.is_empty() && baked.is_empty() {
        return Err(StepExportError::Empty);
    }

    let mut writer = StepWriter::new("ice3-scene");
    writer.header();
    let context = writer.unit_context();
    let identity = writer.placement(Vec3::ZERO, Vec3::Z, Vec3::X);

    let mut root_items: Vec<u64> = Vec::new();
    for ((mesh_key, material_id), instances) in &variant_instances {
        let mesh = &scene.meshes[mesh_key];
        let mut positions = Vec::with_capacity(mesh.vertices.len());
        for vertex in &mesh.vertices {
            positions.push([
                vertex.position[0] * MILLIMETRES_PER_METRE,
                vertex.position[1] * MILLIMETRES_PER_METRE,
                vertex.position[2] * MILLIMETRES_PER_METRE,
            ]);
        }
        let mut triangles = Vec::with_capacity(mesh.indices.len() / 3);
        for chunk in mesh.indices.chunks_exact(3) {
            triangles.push([chunk[0], chunk[1], chunk[2]]);
        }
        let coords = writer.coordinates_list(mesh_key, &positions);
        let set = writer.triangulated_surface_set(mesh_key, coords, positions.len(), &triangles);
        if let Some(material) = material_id.as_deref().and_then(|id| materials.get(id).copied()) {
            writer.styled_item(set, material.base_color, material.opacity);
        }
        let rep = writer.shape_representation(mesh_key, &[set], Some(context));
        let map = writer.representation_map(identity, rep);
        for matrix in instances {
            let (translation, axis, reference) = placement_from_matrix(matrix);
            let target = writer.placement(translation, axis, reference);
            root_items.push(writer.mapped_item(map, target));
        }
    }
    for (name, positions, triangles, material_id) in &baked {
        let coords = writer.coordinates_list(name, positions);
        let set = writer.triangulated_surface_set(name, coords, positions.len(), triangles);
        if let Some(material) = material_id.as_deref().and_then(|id| materials.get(id).copied()) {
            writer.styled_item(set, material.base_color, material.opacity);
        }
        root_items.push(set);
    }
    writer.shape_representation("ice3-scene", &root_items, Some(context));
    Ok(writer.finish())
}

pub(crate) fn is_rigid(matrix: &Mat4) -> bool {
    let (scale, _, _) = matrix.to_scale_rotation_translation();
    (scale - Vec3::ONE).abs().max_element() < 1e-3
}

pub(crate) fn placement_from_matrix(matrix: &Mat4) -> (Vec3, Vec3, Vec3) {
    let (_, rotation, translation) = matrix.to_scale_rotation_translation();
    (
        translation * MILLIMETRES_PER_METRE,
        rotation * Vec3::Z,
        rotation * Vec3::X,
    )
}

pub(crate) fn world_matrices(scene: &EvaluatedScene) -> BTreeMap<String, Mat4> {
    let mut world: BTreeMap<String, Mat4> = BTreeMap::new();
    // Iterative resolution so node order does not matter; a broken parent
    // chain stops the loop without producing partial world matrices.
    for _ in 0..scene.nodes.len().max(1) {
        let mut progressed = false;
        for node in &scene.nodes {
            if world.contains_key(&node.id) {
                continue;
            }
            let local = local_matrix(node);
            match node.parent_id.as_deref() {
                None => {
                    world.insert(node.id.clone(), local);
                    progressed = true;
                }
                Some(parent) if world.contains_key(parent) => {
                    let inherited = world[parent];
                    world.insert(node.id.clone(), inherited * local);
                    progressed = true;
                }
                Some(_) => {}
            }
        }
        if !progressed {
            break;
        }
    }
    world
}

fn local_matrix(node: &EvaluatedSceneNode) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::new(
            node.transform.scale.x,
            node.transform.scale.y,
            node.transform.scale.z,
        ),
        Quat::from_array(node.transform.rotation),
        Vec3::new(
            node.transform.translation.x,
            node.transform.translation.y,
            node.transform.translation.z,
        ),
    )
}

struct StepWriter {
    out: String,
    next: u64,
    root_name: String,
}

impl StepWriter {
    fn new(root_name: &str) -> Self {
        Self {
            out: String::new(),
            next: 1,
            root_name: encode_string(root_name),
        }
    }

    fn id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn line(&mut self, id: u64, body: &str) {
        let _ = writeln!(self.out, "#{id}={body};");
    }

    fn header(&mut self) {
        self.out.push_str("ISO-10303-21;\n");
        self.out.push_str("HEADER;\n");
        self.out.push_str("FILE_DESCRIPTION(('ICE3 Studio tessellated geometry'),'2;1');\n");
        let _ = writeln!(
            self.out,
            "FILE_NAME('{}.step','2026-01-01T00:00:00',(''),(''),'ice3-studio','ice3-studio','');",
            self.root_name
        );
        self.out.push_str("FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF'));\n");
        self.out.push_str("ENDSEC;\n");
        self.out.push_str("DATA;\n");
    }

    /// Complex `GLOBAL_UNIT_ASSIGNED_CONTEXT` with an SI millimetre length unit.
    fn unit_context(&mut self) -> u64 {
        let unit = self.id();
        let context = self.id();
        // `LENGTH_UNIT` / `NAMED_UNIT` are partials of the same complex instance;
        // `*` marks the derived `dimensions` attribute.
        self.line(unit, "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))");
        self.line(
            context,
            &format!(
                "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#{unit}))REPRESENTATION_CONTEXT('',''))"
            ),
        );
        context
    }

    fn coordinates_list(&mut self, name: &str, positions: &[[f32; 3]]) -> u64 {
        let id = self.id();
        let mut body = String::with_capacity(32 + positions.len() * 40);
        let _ = write!(body, "COORDINATES_LIST('{}',{},(", encode_string(name), positions.len());
        for (index, position) in positions.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            let _ = write!(
                body,
                "({},{},{})",
                write_real(position[0] as f64),
                write_real(position[1] as f64),
                write_real(position[2] as f64)
            );
        }
        body.push_str("))");
        self.line(id, &body);
        id
    }

    fn triangulated_surface_set(
        &mut self,
        name: &str,
        coordinates: u64,
        point_count: usize,
        triangles: &[[u32; 3]],
    ) -> u64 {
        let id = self.id();
        let mut body = String::with_capacity(40 + triangles.len() * 16);
        let _ = write!(
            body,
            "TRIANGULATED_SURFACE_SET('{}',#{coordinates},{},$,$,(",
            encode_string(name),
            point_count
        );
        for (index, triangle) in triangles.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            let _ = write!(
                body,
                "({},{},{})",
                triangle[0] + 1,
                triangle[1] + 1,
                triangle[2] + 1
            );
        }
        body.push_str("))");
        self.line(id, &body);
        id
    }

    fn styled_item(&mut self, item: u64, color: [f32; 4], opacity: f32) {
        let colour = self.id();
        let linear = [
            srgb_to_linear(color[0]),
            srgb_to_linear(color[1]),
            srgb_to_linear(color[2]),
        ];
        self.line(
            colour,
            &format!(
                "COLOUR_RGB('',{},{},{})",
                write_real(linear[0] as f64),
                write_real(linear[1] as f64),
                write_real(linear[2] as f64)
            ),
        );
        let fill_colour = self.id();
        self.line(fill_colour, &format!("FILL_AREA_STYLE_COLOUR('',#{colour})"));
        let fill_style = self.id();
        self.line(fill_style, &format!("FILL_AREA_STYLE('',(#{fill_colour}))"));
        let fill_area = self.id();
        self.line(fill_area, &format!("SURFACE_STYLE_FILL_AREA(#{fill_style})"));

        let side_style = if opacity < 1.0 {
            // The fill area carries the colour; a sibling rendering style carries
            // only transparency (`surface_colour = $`), matching how the reader
            // walks a surface side style.
            let transparency = self.id();
            self.line(
                transparency,
                &format!("SURFACE_STYLE_TRANSPARENT({})", write_real((1.0 - opacity) as f64)),
            );
            let rendering = self.id();
            self.line(
                rendering,
                &format!(
                    "SURFACE_STYLE_RENDERING_WITH_PROPERTIES(.NORMAL_SHADING.,$,(#{transparency}))"
                ),
            );
            let side = self.id();
            self.line(
                side,
                &format!("SURFACE_SIDE_STYLE('',(#{fill_area},#{rendering}))"),
            );
            side
        } else {
            let side = self.id();
            self.line(side, &format!("SURFACE_SIDE_STYLE('',(#{fill_area}))"));
            side
        };
        let usage = self.id();
        self.line(usage, &format!("SURFACE_STYLE_USAGE(.BOTH.,#{side_style})"));
        let assignment = self.id();
        self.line(assignment, &format!("PRESENTATION_STYLE_ASSIGNMENT((#{usage}))"));
        let styled = self.id();
        self.line(styled, &format!("STYLED_ITEM('',(#{assignment}),#{item})"));
    }

    fn point(&mut self, position: Vec3) -> u64 {
        let id = self.id();
        self.line(
            id,
            &format!(
                "CARTESIAN_POINT('',({},{},{}))",
                write_real(position.x as f64),
                write_real(position.y as f64),
                write_real(position.z as f64)
            ),
        );
        id
    }

    fn direction(&mut self, direction: Vec3) -> u64 {
        let normalized = direction.normalize_or_zero();
        let id = self.id();
        self.line(
            id,
            &format!(
                "DIRECTION('',({},{},{}))",
                write_real(normalized.x as f64),
                write_real(normalized.y as f64),
                write_real(normalized.z as f64)
            ),
        );
        id
    }

    fn placement(&mut self, translation: Vec3, axis: Vec3, reference: Vec3) -> u64 {
        let location = self.point(translation);
        let axis_id = self.direction(axis);
        let reference_id = self.direction(reference);
        let id = self.id();
        self.line(
            id,
            &format!("AXIS2_PLACEMENT_3D('',#{location},#{axis_id},#{reference_id})"),
        );
        id
    }

    fn representation_map(&mut self, origin: u64, representation: u64) -> u64 {
        let id = self.id();
        self.line(id, &format!("REPRESENTATION_MAP(#{origin},#{representation})"));
        id
    }

    fn mapped_item(&mut self, map: u64, target: u64) -> u64 {
        let id = self.id();
        self.line(id, &format!("MAPPED_ITEM('',#{map},#{target})"));
        id
    }

    fn shape_representation(&mut self, name: &str, items: &[u64], context: Option<u64>) -> u64 {
        let id = self.id();
        let mut body = String::with_capacity(40 + items.len() * 8);
        let _ = write!(body, "SHAPE_REPRESENTATION('{}',(", encode_string(name));
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                body.push(',');
            }
            let _ = write!(body, "#{item}");
        }
        body.push(')');
        match context {
            Some(context) => {
                let _ = write!(body, ",#{context})");
            }
            None => body.push_str(",$)"),
        }
        self.line(id, &body);
        id
    }

    fn finish(mut self) -> String {
        self.out.push_str("ENDSEC;\n");
        self.out.push_str("END-ISO-10303-21;\n");
        self.out
    }
}

fn srgb_to_linear(channel: f32) -> f32 {
    let value = channel.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// A Part 21 REAL literal (always carries a decimal point; finite values only).
fn write_real(value: f64) -> String {
    if !value.is_finite() {
        return "0.".to_string();
    }
    let text = format!("{value:.6}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.")
    }
}

/// ISO 10303-21 string literal body: escape the single quote and encode
/// non-ASCII runs with the `\X2\…\X0\` directive (UTF-16 code units), which
/// the readers in `oxideav-step` / `oxideav-ifc` decode back to Unicode.
pub(crate) fn encode_string(name: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(name.len());
    let mut run = String::new();
    let flush = |out: &mut String, run: &mut String| {
        if run.is_empty() {
            return;
        }
        out.push_str("\\X2\\");
        for unit in run.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push_str("\\X0\\");
        run.clear();
    };
    for character in name.chars() {
        if character == '\'' {
            out.push_str("''");
        } else if character.is_ascii_graphic() || character == ' ' {
            flush(&mut out, &mut run);
            out.push(character);
        } else {
            run.push(character);
        }
    }
    flush(&mut out, &mut run);
    if out.is_empty() {
        out.push_str("part");
    }
    out
}
