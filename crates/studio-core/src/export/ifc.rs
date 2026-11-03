//! IFC4X3 export (BIM).
//!
//! Phase 3b:
//! * spatial hierarchy `IfcProject → IfcSite → IfcBuilding → IfcBuildingStorey`
//!   through `IfcRelAggregates`, with every product contained in the storey via
//!   `IfcRelContainedInSpatialStructure`;
//! * surface styles (`IfcStyledItem` → `IfcSurfaceStyle` →
//!   `IfcSurfaceStyleShading` → `IfcColourRgb`) with transparency;
//! * Unicode names encoded with the `\X2\…\X0\` directive.
//!
//! Geometry: one shared `IfcShapeRepresentation` per `(mesh, material)` variant,
//! `IfcTriangulatedFaceSet` + `IfcCartesianPointList3D` in millimetres, and one
//! `IfcBuildingElementProxy` per node positioned by `IfcLocalPlacement`.
//! Non-rigid nodes are baked to world space.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use glam::{Mat4, Vec3};

use crate::document::{Document, Material};
use crate::export::step::{
    BakedMesh, MILLIMETRES_PER_METRE, StepExportError, encode_string, is_rigid,
    placement_from_matrix, world_matrices,
};
use crate::large_scene::evaluate::EvaluatedScene;

/// Encode an evaluated scene as an IFC4X3 tessellated model.
/// Instances grouped by `(mesh, material)` variant.
type IfcVariants = BTreeMap<(String, Option<String>), Vec<(String, Mat4)>>;

pub fn export_ifc(document: &Document, scene: &EvaluatedScene) -> Result<String, StepExportError> {
    let materials: BTreeMap<&str, &Material> = document
        .materials
        .iter()
        .map(|material| (material.id.as_str(), material))
        .collect();
    let world = world_matrices(scene);

    // Rigid nodes grouped by (mesh, material) so shared geometry is written once.
    let mut variants: IfcVariants = BTreeMap::new();
    // Non-rigid nodes are baked to world space and get their own body.
    let mut baked: Vec<BakedMesh> = Vec::new();

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
        if is_rigid(matrix) {
            variants
                .entry((mesh_key.clone(), material_id))
                .or_default()
                .push((node.name.clone(), *matrix));
        } else {
            let mut positions = Vec::with_capacity(mesh.vertices.len());
            for vertex in &mesh.vertices {
                let point = matrix.transform_point3(Vec3::new(
                    vertex.position[0],
                    vertex.position[1],
                    vertex.position[2],
                )) * MILLIMETRES_PER_METRE;
                positions.push([point.x, point.y, point.z]);
            }
            let triangles = mesh.indices.as_chunks::<3>().0.to_vec();
            baked.push(BakedMesh {
                name: node.name.clone(),
                positions,
                triangles,
                material: material_id,
            });
        }
    }

    if variants.is_empty() && baked.is_empty() {
        return Err(StepExportError::Empty);
    }

    let mut writer = IfcWriter::new();
    writer.header();

    // Units and geometric context.
    let units = writer.id();
    let millimetre = writer.id();
    let context = writer.id();
    let origin_point = writer.point3(0.0, 0.0, 0.0);
    let world_origin = writer.axis_placement(origin_point, None, None);
    let identity_placement = writer.local_placement(world_origin);
    let project = writer.id();
    let project_guid = writer.guid();
    writer.line(
        project,
        &format!(
            "IFCPROJECT('{project_guid}',$,'{}',$,$,$,$,(#{context}),#{units})",
            encode_string("ice3-scene")
        ),
    );
    writer.line(
        context,
        &format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{world_origin},$)"),
    );
    writer.line(units, &format!("IFCUNITASSIGNMENT((#{millimetre}))"));
    writer.line(millimetre, "IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.)");

    // Spatial hierarchy.
    let site = writer.id();
    let site_guid = writer.guid();
    writer.line(
        site,
        &format!(
            "IFCSITE('{site_guid}',$,'{}',$,$,#{identity_placement},$,$,.ELEMENT.,$,$,$,$,$)",
            encode_string("Site")
        ),
    );
    let building = writer.id();
    let building_guid = writer.guid();
    writer.line(
        building,
        &format!(
            "IFCBUILDING('{building_guid}',$,'{}',$,$,#{identity_placement},$,$,.ELEMENT.,$,$,$)",
            encode_string("Building")
        ),
    );
    let storey = writer.id();
    let storey_guid = writer.guid();
    writer.line(
        storey,
        &format!(
            "IFCBUILDINGSTOREY('{storey_guid}',$,'{}',$,$,#{identity_placement},$,$,.ELEMENT.,0.)",
            encode_string("Storey")
        ),
    );

    let mut products: Vec<(String, u64, u64)> = Vec::new();
    let mut product_ids: Vec<u64> = Vec::new();

    for ((mesh_key, material_id), instances) in &variants {
        let mesh = &scene.meshes[mesh_key];
        let mut positions = Vec::with_capacity(mesh.vertices.len());
        for vertex in &mesh.vertices {
            positions.push([
                vertex.position[0] * MILLIMETRES_PER_METRE,
                vertex.position[1] * MILLIMETRES_PER_METRE,
                vertex.position[2] * MILLIMETRES_PER_METRE,
            ]);
        }
        let triangles = mesh.indices.as_chunks::<3>().0.to_vec();
        let face_set = writer.body(&positions, &triangles);
        if let Some(material) = material_id
            .as_deref()
            .and_then(|id| materials.get(id).copied())
        {
            writer.style(face_set, material.base_color, material.opacity);
        }
        let pds = writer.product_shape(face_set, context);
        for (name, matrix) in instances {
            let placement = writer.placement(matrix);
            products.push((name.clone(), placement, pds));
        }
    }
    for baked_mesh in &baked {
        let face_set = writer.body(&baked_mesh.positions, &baked_mesh.triangles);
        if let Some(material) = baked_mesh
            .material
            .as_deref()
            .and_then(|id| materials.get(id).copied())
        {
            writer.style(face_set, material.base_color, material.opacity);
        }
        let pds = writer.product_shape(face_set, context);
        let placement = writer.local_placement(identity_placement);
        products.push((baked_mesh.name.clone(), placement, pds));
    }

    for (name, placement, pds) in &products {
        let product = writer.id();
        let guid = writer.guid();
        writer.line(
            product,
            &format!(
                "IFCBUILDINGELEMENTPROXY('{guid}',$,'{}',$,$,#{placement},#{pds},$)",
                encode_string(name)
            ),
        );
        product_ids.push(product);
    }

    // Aggregation and containment.
    writer.aggregate(project, &[site]);
    writer.aggregate(site, &[building]);
    writer.aggregate(building, &[storey]);
    writer.contain(storey, &product_ids);

    Ok(writer.finish())
}

struct IfcWriter {
    out: String,
    next: u64,
    seed: u64,
}

impl IfcWriter {
    fn new() -> Self {
        Self {
            out: String::new(),
            next: 1,
            seed: 1,
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
        self.out
            .push_str("FILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n");
        self.out.push_str(
            "FILE_NAME('ice3-scene.ifc','2026-01-01T00:00:00',(''),(''),'ice3-studio','ice3-studio','');\n",
        );
        self.out.push_str("FILE_SCHEMA(('IFC4X3'));\n");
        self.out.push_str("ENDSEC;\n");
        self.out.push_str("DATA;\n");
    }

    fn finish(mut self) -> String {
        self.out.push_str("ENDSEC;\n");
        self.out.push_str("END-ISO-10303-21;\n");
        self.out
    }

    /// Deterministic 22-character IFC GlobalId.
    fn guid(&mut self) -> String {
        const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
        let mut value = self.seed;
        self.seed += 1;
        let mut out = String::with_capacity(22);
        out.push('0');
        for _ in 1..22 {
            out.push(ALPHABET[(value % 64) as usize] as char);
            value /= 64;
        }
        out
    }

    fn point3(&mut self, x: f64, y: f64, z: f64) -> u64 {
        let id = self.id();
        self.line(
            id,
            &format!("IFCCARTESIANPOINT(({},{},{}))", real(x), real(y), real(z)),
        );
        id
    }

    fn direction(&mut self, x: f64, y: f64, z: f64) -> u64 {
        let id = self.id();
        self.line(
            id,
            &format!("IFCDIRECTION(({},{},{}))", real(x), real(y), real(z)),
        );
        id
    }

    fn axis_placement(&mut self, location: u64, axis: Option<u64>, reference: Option<u64>) -> u64 {
        let id = self.id();
        let axis_text = axis
            .map(|value| format!("#{value}"))
            .unwrap_or_else(|| "$".to_string());
        let reference_text = reference
            .map(|value| format!("#{value}"))
            .unwrap_or_else(|| "$".to_string());
        self.line(
            id,
            &format!("IFCAXIS2PLACEMENT3D(#{location},{axis_text},{reference_text})"),
        );
        id
    }

    fn local_placement(&mut self, relative: u64) -> u64 {
        let id = self.id();
        self.line(id, &format!("IFCLOCALPLACEMENT($,#{relative})"));
        id
    }

    fn placement(&mut self, matrix: &Mat4) -> u64 {
        let (translation, axis, reference) = placement_from_matrix(matrix);
        let location = self.point3(
            translation.x as f64,
            translation.y as f64,
            translation.z as f64,
        );
        let normal = axis.normalize_or_zero();
        let axis_id = self.direction(normal.x as f64, normal.y as f64, normal.z as f64);
        let ref_dir = reference.normalize_or_zero();
        let ref_id = self.direction(ref_dir.x as f64, ref_dir.y as f64, ref_dir.z as f64);
        let axis_placement = self.axis_placement(location, Some(axis_id), Some(ref_id));
        self.local_placement(axis_placement)
    }

    /// Emit a tessellated body and return the face-set id.
    fn body(&mut self, positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> u64 {
        let coordinates = self.id();
        let mut text = String::with_capacity(32 + positions.len() * 32);
        text.push_str("IFCCARTESIANPOINTLIST3D((");
        for (index, position) in positions.iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            let _ = write!(
                text,
                "({},{},{})",
                real(position[0] as f64),
                real(position[1] as f64),
                real(position[2] as f64)
            );
        }
        text.push_str("))");
        self.line(coordinates, &text);

        let face_set = self.id();
        let mut faces = String::with_capacity(32 + triangles.len() * 16);
        let _ = write!(faces, "IFCTRIANGULATEDFACESET(#{coordinates},$,.T.,(");
        for (index, triangle) in triangles.iter().enumerate() {
            if index > 0 {
                faces.push(',');
            }
            let _ = write!(
                faces,
                "({},{},{})",
                triangle[0] + 1,
                triangle[1] + 1,
                triangle[2] + 1
            );
        }
        faces.push_str("),$)");
        self.line(face_set, &faces);
        face_set
    }

    fn product_shape(&mut self, face_set: u64, context: u64) -> u64 {
        let representation = self.id();
        self.line(
            representation,
            &format!("IFCSHAPEREPRESENTATION(#{context},'Body','Tessellation',(#{face_set}))"),
        );
        let pds = self.id();
        self.line(
            pds,
            &format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{representation}))"),
        );
        pds
    }

    fn style(&mut self, item: u64, color: [f32; 4], opacity: f32) {
        let colour = self.id();
        self.line(
            colour,
            &format!(
                "IFCCOLOURRGB($,{},{},{})",
                real(color[0].clamp(0.0, 1.0) as f64),
                real(color[1].clamp(0.0, 1.0) as f64),
                real(color[2].clamp(0.0, 1.0) as f64)
            ),
        );
        let shading = self.id();
        self.line(
            shading,
            &format!(
                "IFCSURFACESTYLESHADING(#{colour},{})",
                real((1.0 - opacity).clamp(0.0, 1.0) as f64)
            ),
        );
        let style = self.id();
        self.line(
            style,
            &format!("IFCSURFACESTYLE('Material',.BOTH.,(#{shading}))"),
        );
        let styled = self.id();
        self.line(styled, &format!("IFCSTYLEDITEM(#{item},(#{style}),$)"));
    }

    fn aggregate(&mut self, parent: u64, children: &[u64]) {
        let id = self.id();
        let guid = self.guid();
        let mut refs = String::new();
        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                refs.push(',');
            }
            let _ = write!(refs, "#{child}");
        }
        self.line(
            id,
            &format!("IFCRELAGGREGATES('{guid}',$,$,$,#{parent},({refs}))"),
        );
    }

    fn contain(&mut self, structure: u64, elements: &[u64]) {
        let id = self.id();
        let guid = self.guid();
        let mut refs = String::new();
        for (index, element) in elements.iter().enumerate() {
            if index > 0 {
                refs.push(',');
            }
            let _ = write!(refs, "#{element}");
        }
        self.line(
            id,
            &format!("IFCRELCONTAINEDINSPATIALSTRUCTURE('{guid}',$,$,$,({refs}),#{structure})"),
        );
    }
}

/// Part 21 REAL literal.
fn real(value: f64) -> String {
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
