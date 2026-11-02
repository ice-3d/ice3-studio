use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import")
        .join(name)
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args(args)
        .output()
        .unwrap()
}

fn cli_with_env(args: &[&str], environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_studio-cli"));
    command.args(args);
    for (name, value) in environment {
        command.env(name, value);
    }
    command.output().unwrap()
}

fn cli_json(args: &[&str]) -> Value {
    let output = cli(args);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_budget(root: &Path, name: &str, soft: u64, hard: u64) -> PathBuf {
    let path = root.join(name);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "id": name,
            "limits": [{
                "id": "triangleCount",
                "label": "Triangles",
                "softLimit": soft,
                "hardLimit": hard
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

#[test]
fn build_emits_edges_report_and_budget() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    let result = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let parsed = studio_assets::read_glb(&output).unwrap();
    assert_eq!(
        parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"],
        12
    );
    assert_eq!(
        parsed.json["asset"]["extras"]["ice3"]["preparation"]["creaseAngleDegrees"],
        30.0
    );
    let report_bytes = std::fs::read(&report).unwrap();
    let report_text = String::from_utf8(report_bytes.clone()).unwrap();
    assert!(!report_text.contains(root.path().to_str().unwrap()));
    let report: Value = serde_json::from_slice(&report_bytes).unwrap();
    assert_eq!(report["kind"], "ice3-asset-build-report");
    assert_eq!(report["budget"]["status"], "pass");
    assert_eq!(report["metrics"]["triangleCount"], 12);
    for field in [
        "nodeCount",
        "maxNodeDepth",
        "meshCount",
        "primitiveCount",
        "triangleCount",
        "edgeSegmentCount",
        "materialCount",
        "selectionGroupCount",
    ] {
        assert_eq!(
            report["metrics"][field],
            parsed.json["asset"]["extras"]["ice3"]["stats"][field]
        );
    }
    assert_eq!(
        report["sourceDigest"],
        parsed.json["asset"]["extras"]["ice3"]["sourceDigest"]
    );
    assert_eq!(
        report["contentDigest"],
        format!(
            "sha256:{}",
            parsed.json["asset"]["extras"]["ice3"]["contentDigest"]["value"]
                .as_str()
                .unwrap()
        )
    );
}

#[test]
fn import_has_no_edges_while_build_does() {
    let root = tempfile::tempdir().unwrap();
    let import_output = root.path().join("import.glb");
    let build_output = root.path().join("build.glb");
    let import = cli_json(&[
        "import",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        import_output.to_str().unwrap(),
        "--asset-id",
        "cube",
    ]);
    let build = cli_json(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        build_output.to_str().unwrap(),
        "--asset-id",
        "cube",
    ]);
    assert_eq!(import["metrics"]["edgeSegmentCount"], 0);
    assert_eq!(build["metrics"]["edgeSegmentCount"], 12);
}

#[test]
fn inspect_source_and_prepared_use_the_same_fields() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    cli_json(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
    ]);
    let source = cli_json(&[
        "inspect",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--json",
    ]);
    let prepared = cli_json(&["inspect", output.to_str().unwrap(), "--json"]);
    assert_eq!(
        source["metrics"]["triangleCount"],
        prepared["metrics"]["triangleCount"]
    );
    assert_eq!(
        source["metrics"]["edgeSegmentCount"],
        prepared["metrics"]["edgeSegmentCount"]
    );
    assert_eq!(source["kind"], "ice3-asset-build-report");
    assert_eq!(prepared["kind"], "ice3-asset-build-report");
}

#[test]
fn skip_edges_is_reported_and_json_controls_stdout_formatting() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let pretty = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--skip-edges",
    ]);
    assert!(
        pretty.status.success(),
        "{}",
        String::from_utf8_lossy(&pretty.stderr)
    );
    let pretty_stdout = String::from_utf8_lossy(&pretty.stdout);
    assert!(pretty_stdout.lines().count() > 1);
    let report: Value = serde_json::from_str(&pretty_stdout).unwrap();
    assert_eq!(report["metrics"]["edgeSegmentCount"], 0);
    assert_eq!(report["metrics"]["materialCount"], 5);
    assert_eq!(report["preparation"]["edgesPrepared"], false);

    let compact = cli(&[
        "inspect",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--json",
    ]);
    assert!(
        compact.status.success(),
        "{}",
        String::from_utf8_lossy(&compact.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&compact.stdout).lines().count(), 1);
    serde_json::from_slice::<Value>(&compact.stdout).unwrap();
}

#[test]
fn hard_budget_does_not_publish_and_preserves_existing_files() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    std::fs::write(&output, b"old-output").unwrap();
    std::fs::write(&report, b"old-report").unwrap();
    let budget = write_budget(root.path(), "hard.json", 0, 0);
    let result = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--budget",
        budget.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
        "--force",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_BUDGET_EXCEEDED"));
    assert_eq!(std::fs::read(output).unwrap(), b"old-output");
    assert_eq!(std::fs::read(report).unwrap(), b"old-report");
}

#[test]
fn warning_as_error_blocks_publication_but_soft_budget_otherwise_works() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    let budget = write_budget(root.path(), "soft.json", 0, 100);
    let ok = cli_json(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--budget",
        budget.to_str().unwrap(),
        "--report",
        report.to_str().unwrap(),
    ]);
    assert_eq!(ok["budget"]["status"], "warning");
    assert!(report.exists());

    let blocked_output = root.path().join("blocked.glb");
    let blocked_report = root.path().join("blocked.report.json");
    let blocked = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        blocked_output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--budget",
        budget.to_str().unwrap(),
        "--report",
        blocked_report.to_str().unwrap(),
        "--warnings-as-errors",
    ]);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("ICE3_STUDIO_ASSET_BUDGET_WARNING"));
    assert!(!blocked_output.exists());
    assert!(!blocked_report.exists());
}

#[test]
fn invalid_report_path_does_not_publish_output() {
    let root = tempfile::tempdir().unwrap();
    let not_directory = root.path().join("not-directory");
    std::fs::write(&not_directory, b"file").unwrap();
    let output = root.path().join("cube.glb");
    let report = not_directory.join("report.json");
    let result = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED")
    );
    assert!(!output.exists());
}

#[test]
fn no_clobber_and_force_control_both_outputs() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    std::fs::write(&output, b"old-output").unwrap();
    std::fs::write(&report, b"old-report").unwrap();
    let source = fixture("gltf/prepare-cube.gltf");
    let args = [
        "build",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report.to_str().unwrap(),
        "--force",
    ];
    let result = cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_ne!(std::fs::read(&output).unwrap(), b"old-output");
    assert_ne!(std::fs::read(&report).unwrap(), b"old-report");

    let output_again = root.path().join("again.glb");
    let report_again = root.path().join("again.report.json");
    std::fs::write(&output_again, b"keep-output").unwrap();
    let rejected = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output_again.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report_again.to_str().unwrap(),
    ]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ICE3_STUDIO_ASSET_OUTPUT_EXISTS"));
    assert_eq!(std::fs::read(output_again).unwrap(), b"keep-output");
    assert!(!report_again.exists());

    let output_report_conflict = root.path().join("report-conflict.glb");
    let report_conflict = root.path().join("report-conflict.report.json");
    std::fs::write(&report_conflict, b"keep-report").unwrap();
    let rejected = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output_report_conflict.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report_conflict.to_str().unwrap(),
    ]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("ICE3_STUDIO_ASSET_OUTPUT_EXISTS"));
    assert!(!output_report_conflict.exists());
    assert_eq!(std::fs::read(report_conflict).unwrap(), b"keep-report");
}

#[test]
fn invalid_crease_and_budget_are_stable_asset_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let invalid_crease = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--crease-angle",
        "181",
    ]);
    assert!(!invalid_crease.status.success());
    assert!(
        String::from_utf8_lossy(&invalid_crease.stderr)
            .contains("ICE3_STUDIO_ASSET_INVALID_GEOMETRY")
    );

    let invalid_budget = root.path().join("invalid.json");
    std::fs::write(
        &invalid_budget,
        br#"{"id":"bad","limits":[{"id":"triangleCount","label":"Triangles","softLimit":-1,"hardLimit":2}]}"#,
    )
    .unwrap();
    let invalid = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--budget",
        invalid_budget.to_str().unwrap(),
    ]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("ICE3_STUDIO_ASSET_INVALID_BUDGET"));
    assert!(!output.exists());
}

#[cfg(feature = "jt")]
#[test]
fn invalid_build_and_inspect_options_are_rejected_before_the_jt_converter_starts() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    let marker = root.path().join("converter-started");
    let invalid_budget = root.path().join("invalid.json");
    std::fs::write(
        &invalid_budget,
        br#"{"id":"bad","limits":[{"id":"triangleCount","label":"Triangles","softLimit":-1,"hardLimit":2}]}"#,
    )
    .unwrap();
    let converter = fixture("jt/start-marker-converter.sh");

    let result = cli_with_env(
        &[
            "build",
            fixture("jt/synthetic-contract.jt").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "jt-prevalidated",
            "--budget",
            invalid_budget.to_str().unwrap(),
        ],
        &[
            ("ICE3_JT_CONVERTER", converter.as_path()),
            ("ICE3_JT_START_MARKER", marker.as_path()),
        ],
    );

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_INVALID_BUDGET"));
    assert!(
        !marker.exists(),
        "converter must not start before option validation"
    );
    assert!(!output.exists());

    let inspect = cli_with_env(
        &[
            "inspect",
            fixture("jt/synthetic-contract.jt").to_str().unwrap(),
            "--budget",
            invalid_budget.to_str().unwrap(),
        ],
        &[
            ("ICE3_JT_CONVERTER", converter.as_path()),
            ("ICE3_JT_START_MARKER", marker.as_path()),
        ],
    );

    assert!(!inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stderr).contains("ICE3_STUDIO_ASSET_INVALID_BUDGET"));
    assert!(
        !marker.exists(),
        "converter must not start before inspect option validation"
    );
}

#[test]
fn prepared_inspect_rejects_source_preparation_options() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    cli_json(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--json",
    ]);

    for extra in [
        vec!["--skip-edges"],
        vec!["--crease-angle", "45"],
        vec!["--budget", "unrestricted"],
    ] {
        let mut args = vec!["inspect", output.to_str().unwrap(), "--json"];
        args.extend(extra);
        let result = cli(&args);
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_INVALID_OPTIONS")
        );
    }
}

#[test]
fn build_and_inspect_support_a_non_gltf_adapter() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("plant.glb");
    let source = fixture("obj/synthetic-plant.obj");
    let build = cli_json(&[
        "build",
        source.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "obj-build",
        "--json",
    ]);
    let inspect = cli_json(&["inspect", source.to_str().unwrap(), "--json"]);
    assert!(build["metrics"]["triangleCount"].as_u64().unwrap() > 0);
    assert_eq!(
        build["metrics"]["triangleCount"],
        inspect["metrics"]["triangleCount"]
    );
}

#[cfg(unix)]
#[test]
fn failed_second_publish_removes_new_report_and_preserves_output_path() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    let missing_target = root.path().join("missing-target.glb");
    symlink(&missing_target, &output).unwrap();

    let result = cli(&[
        "build",
        fixture("gltf/prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "cube",
        "--report",
        report.to_str().unwrap(),
    ]);

    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("ICE3_STUDIO_ASSET_OUTPUT_EXISTS"));
    assert_eq!(std::fs::read_link(&output).unwrap(), missing_target);
    assert!(!report.exists());
}
