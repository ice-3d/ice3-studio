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
    assert!(canonical.materials[0].double_sided);
    assert_eq!(
        canonical.materials[0].surface.as_ref().unwrap().metallic,
        0.8
    );
    assert_eq!(
        canonical.materials[0]
            .surface
            .as_ref()
            .unwrap()
            .normal_texture
            .as_ref()
            .unwrap()
            .scale,
        1.5
    );
    assert_eq!(
        canonical.meshes[0].primitives[0]
            .uvs
            .as_ref()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        canonical.resources,
        vec![studio_assets::canonical::CanonicalResource {
            digest: "a".repeat(64),
            media_type: "image/png".into(),
            bytes: vec![1, 2, 3],
        }]
    );
}
