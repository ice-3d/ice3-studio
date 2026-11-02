use std::path::PathBuf;
use std::process::ExitCode;

use studio_import::{
    FbxImporter, GltfImporter, ObjImporter, SourceImporterRegistry, UsdImporter,
    execute_worker_request,
};

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
    let registry = SourceImporterRegistry::try_new(vec![
        Box::new(GltfImporter::new()),
        Box::new(ObjImporter::new()),
        Box::new(FbxImporter::new()),
        Box::new(UsdImporter::new()),
    ])?;
    execute_worker_request(request_path, &registry)
}
