use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::ImportError;

const SUPERVISOR_FLAG: &str = "--internal-jt-converter-supervisor";
const START_BYTES: &[u8] = b"start\n";

pub fn run_jt_converter_supervisor(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<bool, ImportError> {
    let arguments = arguments.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let Some(flag) = arguments.first() else {
        return Ok(false);
    };
    if flag != SUPERVISOR_FLAG {
        return Ok(false);
    }
    let [
        _,
        converter_flag,
        converter,
        input_flag,
        input,
        output_flag,
        output,
    ] = arguments.as_slice()
    else {
        return Err(ImportError::WorkerProtocolInvalid(
            "invalid JT converter supervisor arguments".into(),
        ));
    };
    if converter_flag != "--converter" || input_flag != "--input" || output_flag != "--output" {
        return Err(ImportError::WorkerProtocolInvalid(
            "invalid JT converter supervisor arguments".into(),
        ));
    }

    let mut start = [0_u8; START_BYTES.len()];
    std::io::stdin()
        .read_exact(&mut start)
        .map_err(|_| converter_failed())?;
    if start != START_BYTES {
        return Err(converter_failed());
    }

    let status = Command::new(converter)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|_| converter_failed())?;
    if status.success() {
        Ok(true)
    } else {
        Err(converter_failed())
    }
}

fn converter_failed() -> ImportError {
    ImportError::SourceDiagnostic {
        code: "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED".into(),
        message: "JT converter failed".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_false_for_unrelated_arguments() {
        assert!(!run_jt_converter_supervisor([OsString::from("serve")]).unwrap());
    }
}
