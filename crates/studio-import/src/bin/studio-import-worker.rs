use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(feature = "fbx")]
use studio_import::FbxImporter;
#[cfg(feature = "ifc")]
use studio_import::IfcImporter;
#[cfg(feature = "jt")]
use studio_import::JtImporter;
#[cfg(feature = "step")]
use studio_import::StepImporter;
#[cfg(feature = "usd")]
use studio_import::UsdImporter;
use studio_import::{GltfImporter, ObjImporter, SourceImporterRegistry, execute_worker_request};

fn main() -> ExitCode {
    match worker_main(std::env::args_os().skip(1).map(PathBuf::from).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {}", error.code(), error);
            ExitCode::FAILURE
        }
    }
}

fn worker_main(arguments: Vec<PathBuf>) -> Result<(), studio_import::ImportError> {
    let [flag, request_path] = arguments.as_slice() else {
        return Err(studio_import::ImportError::WorkerProtocolInvalid(
            "expected --internal-import-worker REQUEST_PATH".into(),
        ));
    };
    if flag != "--internal-import-worker" {
        return Err(studio_import::ImportError::WorkerProtocolInvalid(
            "expected --internal-import-worker".into(),
        ));
    }
    #[allow(unused_mut)]
    let mut importers: Vec<Box<dyn studio_import::SourceImporter>> =
        vec![Box::new(GltfImporter::new()), Box::new(ObjImporter::new())];
    #[cfg(feature = "fbx")]
    importers.push(Box::new(FbxImporter::new()));
    #[cfg(feature = "usd")]
    importers.push(Box::new(UsdImporter::new()));
    #[cfg(feature = "step")]
    importers.push(Box::new(StepImporter::new()));
    #[cfg(feature = "ifc")]
    importers.push(Box::new(IfcImporter::new()));
    #[cfg(feature = "jt")]
    importers.push(Box::new(JtImporter::new()));
    let registry = SourceImporterRegistry::try_new(importers)?;
    execute_worker_request(request_path, &registry)
}
