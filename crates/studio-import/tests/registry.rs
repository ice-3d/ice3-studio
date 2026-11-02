use std::path::Path;

use studio_import::{
    ImportError, ImportRequest, SourceCapabilities, SourceFormat, SourceImporter,
    SourceImporterRegistry, SourceScene,
};

struct FakeImporter {
    id: &'static str,
    extensions: &'static [&'static str],
}

impl SourceImporter for FakeImporter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: false,
            material_colors: true,
            instances: false,
            semantic_metadata: true,
        }
    }

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        Ok(SourceScene {
            asset_id: request.asset_id.clone(),
            source_format: SourceFormat::Obj,
            nodes: Vec::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            warnings: Vec::new(),
            asset_extras: serde_json::Value::Null,
        })
    }
}

#[test]
fn resolves_an_importer_by_extension() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(FakeImporter {
        id: "obj",
        extensions: &[".obj"],
    })])
    .unwrap();
    assert_eq!(
        registry.resolve(Path::new("plant.OBJ"), None).unwrap().id(),
        "obj"
    );
}

#[test]
fn rejects_duplicate_importer_ids() {
    let first = Box::new(FakeImporter {
        id: "obj",
        extensions: &[".obj"],
    });
    let second = Box::new(FakeImporter {
        id: "obj",
        extensions: &[".objx"],
    });
    let error = SourceImporterRegistry::try_new(vec![first, second])
        .err()
        .unwrap();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_REGISTRY_INVALID");
}

#[test]
fn rejects_an_explicit_adapter_with_the_wrong_extension() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(FakeImporter {
        id: "obj",
        extensions: &[".obj"],
    })])
    .unwrap();
    let error = registry
        .resolve(Path::new("plant.glb"), Some("obj"))
        .err()
        .unwrap();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE");
}
