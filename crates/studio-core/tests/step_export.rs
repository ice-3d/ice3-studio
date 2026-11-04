//! Round-trip validation for the AP242 tessellated STEP writer.
//!
//! These tests use `oxideav-step`'s reader as an independent oracle: a file we
//! emit must parse back into the expected triangle count and geometry. This
//! keeps the writer honest without hand-waving about STEP validity.

use std::collections::BTreeMap;

use studio_core::document::{Document, Material};
use studio_core::export::step::{
    StepExportError, StepMeshBucket, buckets_from_scene, encode_step, export_step,
    export_step_from_scene,
};
use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::ids::MaterialId;
use studio_core::large_scene::evaluate::{EvaluatedScene, EvaluatedSceneNode, LargeSceneStats};
use studio_core::math::{Transform, Vec3};

fn triangle_mesh() -> EvaluatedMesh {
    EvaluatedMesh {
        vertices: vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [0.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2],
        bounds: Bounds {
            min: Vec3::default(),
            max: Vec3::default(),
        },
    }
}

fn cube_bucket() -> StepMeshBucket {
    let mut positions = Vec::new();
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                positions.push([x as f32, y as f32, z as f32]);
            }
        }
    }
    // Cube vertex order: (x, y, z) with index = x + 2*y + 4*z.
    let triangles = vec![
        [0, 1, 3],
        [0, 3, 2], // z = 0
        [4, 6, 7],
        [4, 7, 5], // z = 1
        [0, 4, 5],
        [0, 5, 1], // y = 0
        [2, 3, 7],
        [2, 7, 6], // y = 1
        [0, 2, 6],
        [0, 6, 4], // x = 0
        [1, 5, 7],
        [1, 7, 3], // x = 1
    ];
    StepMeshBucket {
        name: "cube".to_string(),
        positions,
        triangles,
    }
}

#[test]
fn encodes_ap242_tessellated_and_parses_back() {
    let step = encode_step(&[cube_bucket()], "cube");
    assert!(step.starts_with("ISO-10303-21;"));
    assert!(step.contains("TRIANGULATED_SURFACE_SET"));

    let model = oxideav_step::read_step(step.as_bytes()).expect("emitted STEP parses");
    assert_eq!(model.triangle_count(), 12, "all 12 cube triangles survive");
    assert!(
        !model.parts.is_empty(),
        "the shape representation becomes a part"
    );
}

#[test]
fn bakes_world_transform_in_millimetres() {
    let mut meshes = BTreeMap::new();
    meshes.insert("mesh".to_string(), triangle_mesh());
    let scene = EvaluatedScene {
        nodes: vec![EvaluatedSceneNode {
            id: "node".to_string(),
            owner_id: "node".to_string(),
            name: "node".to_string(),
            parent_id: None,
            transform: Transform {
                translation: Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                ..Transform::IDENTITY
            },
            material_id: None,
            mesh_key: Some("mesh".to_string()),
            resident_geometry_id: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        meshes,
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    };

    let buckets = buckets_from_scene(&scene);
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].positions.len(), 3);
    // (0,0,0) local -> (1,2,3) m -> (1000,2000,3000) mm.
    assert!((buckets[0].positions[0][0] - 1000.0).abs() < 1e-3);
    assert!((buckets[0].positions[0][1] - 2000.0).abs() < 1e-3);
    assert!((buckets[0].positions[0][2] - 3000.0).abs() < 1e-3);
}

#[test]
fn empty_scene_is_rejected() {
    let scene = EvaluatedScene {
        nodes: Vec::new(),
        meshes: BTreeMap::new(),
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    };
    assert!(matches!(
        export_step_from_scene(&scene),
        Err(StepExportError::Empty)
    ));
}

/// Opt-in: parse a real exported STEP file when `STUDIO_STEP_SAMPLE` points at
/// one. No-op in normal test runs, so CI stays hermetic.
#[test]
fn parses_exported_sample_when_available() {
    let Ok(path) = std::env::var("STUDIO_STEP_SAMPLE") else {
        return;
    };
    let bytes = std::fs::read(path).expect("sample STEP file");
    let model = oxideav_step::read_step(&bytes).expect("sample STEP parses");
    assert!(model.triangle_count() > 0, "sample carries triangles");
    assert!(!model.parts.is_empty(), "sample carries parts");
    assert!(
        (model.length_unit_metres - 0.001).abs() < 1e-9,
        "sample declares millimetres, got {}",
        model.length_unit_metres
    );
}

#[test]
fn exports_units_colours_and_instancing() {
    let material_id = MaterialId::new("mat", 1).expect("valid id");
    let mut document = Document::new("test");
    document.materials.push(Material {
        id: material_id.clone(),
        name: "red translucent".to_string(),
        base_color: [1.0, 0.0, 0.0, 1.0],
        opacity: 0.5,
        edge_color: None,
        shading: None,
        surface: None,
    });

    let mut meshes = BTreeMap::new();
    meshes.insert("mesh".to_string(), triangle_mesh());
    let scene = EvaluatedScene {
        nodes: vec![
            instance_node("a", 0.0, Some(&material_id)),
            instance_node("b", 5.0, Some(&material_id)),
        ],
        meshes,
        resident_geometry_ids: BTreeMap::new(),
        stats: LargeSceneStats::default(),
    };

    let step = export_step(&document, &scene).expect("phase 2 export");
    assert!(step.contains("GLOBAL_UNIT_ASSIGNED_CONTEXT"));
    assert!(step.contains("COLOUR_RGB"));
    assert!(step.contains("MAPPED_ITEM"));

    let model = oxideav_step::read_step(step.as_bytes()).expect("phase 2 STEP parses");
    // Millimetres are declared through the unit context.
    assert!((model.length_unit_metres - 0.001).abs() < 1e-9);
    // Instancing: one shared mesh counted once, placed twice.
    assert_eq!(
        model.triangle_count(),
        1,
        "shared geometry is not duplicated"
    );
    let occurrence_count: usize = model.parts.iter().map(|part| part.children.len()).sum();
    assert_eq!(
        occurrence_count, 2,
        "two instances reference the shared part"
    );
    // Colour + transparency survive the round trip.
    let colour = model
        .parts
        .iter()
        .flat_map(|part| &part.shapes)
        .find_map(|shape| shape.colour);
    let colour = colour.expect("shape carries a style");
    assert!((colour[0] - 1.0).abs() < 1e-3 && colour[1] < 1e-3);
    assert!(
        (colour[3] - 0.5).abs() < 1e-3,
        "alpha from transparency, got {}",
        colour[3]
    );
}

fn instance_node(id: &str, x: f32, material: Option<&MaterialId>) -> EvaluatedSceneNode {
    EvaluatedSceneNode {
        id: id.to_string(),
        owner_id: id.to_string(),
        name: id.to_string(),
        parent_id: None,
        transform: Transform {
            translation: Vec3 { x, y: 0.0, z: 0.0 },
            ..Transform::IDENTITY
        },
        material_id: material.cloned(),
        mesh_key: Some("mesh".to_string()),
        resident_geometry_id: None,
        metadata: BTreeMap::new(),
        visible: true,
    }
}

#[test]
fn unicode_names_round_trip() {
    let mut bucket = cube_bucket();
    bucket.name = "泵房".to_string();
    let step = encode_step(&[bucket], "scene");
    assert!(step.contains("\\X2\\"), "non-ASCII is escaped");
    let model = oxideav_step::read_step(step.as_bytes()).expect("unicode STEP parses");
    let name = model
        .parts
        .iter()
        .flat_map(|part| &part.shapes)
        .find_map(|shape| shape.name.clone());
    assert_eq!(name.as_deref(), Some("泵房"));
}
