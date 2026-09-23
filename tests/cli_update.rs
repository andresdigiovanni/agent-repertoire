use assert_cmd::Command;
use predicates::str::contains;
use std::path::PathBuf;

fn rep(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("AGENT_REPERTOIRE_HOME", home);
    cmd
}

fn yaml_file(dir: &std::path::Path, description: &str) -> PathBuf {
    let path = dir.join("tool.yaml");
    std::fs::write(
        &path,
        format!(
            "name: cli_upd\ndescription: \"{description}\"\nlanguage: bash\nentrypoint: run.sh\nkeywords:\n  - test\n"
        ),
    )
    .unwrap();
    path
}

fn script_file(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("run.sh");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn create_then_update_then_source_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let yaml = yaml_file(work.path(), "first version");
    let _script = script_file(work.path(), "#!/usr/bin/env bash\necho '{\"n\": 1}'\n");

    rep(tmp.path())
        .args(["create", "--file"])
        .arg(&yaml)
        .assert()
        .success()
        .stdout(contains("created cli_upd (version 1)"));

    let script2 = script_file(work.path(), "#!/usr/bin/env bash\necho '{\"n\": 2}'\n");
    rep(tmp.path())
        .args(["update", "cli_upd", "--script"])
        .arg(&script2)
        .assert()
        .success()
        .stdout(contains("updated cli_upd (version 2)"));

    rep(tmp.path())
        .args(["source", "cli_upd"])
        .assert()
        .success()
        .stdout(contains("\"n\": 2"));

    rep(tmp.path())
        .args(["source", "cli_upd", "--yaml"])
        .assert()
        .success()
        .stdout(contains("first version"));

    let yaml2 = yaml_file(work.path(), "second version");
    rep(tmp.path())
        .args(["update", "cli_upd", "--file"])
        .arg(&yaml2)
        .assert()
        .success()
        .stdout(contains("version 3"));

    rep(tmp.path())
        .args(["source", "cli_upd", "--yaml"])
        .assert()
        .success()
        .stdout(contains("second version"));
}

#[test]
fn update_requires_script_or_file() {
    let tmp = tempfile::tempdir().unwrap();
    rep(tmp.path())
        .args(["update", "cli_anything"])
        .assert()
        .failure()
        .stderr(contains("--script"));
}

#[test]
fn source_unknown_tool_fails() {
    let tmp = tempfile::tempdir().unwrap();
    rep(tmp.path())
        .args(["source", "ghost"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn source_yaml_round_trips_into_update_file() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let yaml = yaml_file(work.path(), "round trip meta");
    let _script = script_file(work.path(), "#!/usr/bin/env bash\necho '{\"n\": 1}'\n");

    rep(tmp.path())
        .args(["create", "--file"])
        .arg(&yaml)
        .assert()
        .success()
        .stdout(contains("created cli_upd (version 1)"));

    let assert = rep(tmp.path())
        .args(["source", "cli_upd", "--yaml"])
        .assert()
        .success();
    let generated_yaml = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(generated_yaml.contains("cli_upd"));
    let generated = work.path().join("generated.yaml");
    std::fs::write(&generated, generated_yaml).unwrap();

    rep(tmp.path())
        .args(["update", "cli_upd", "--file"])
        .arg(&generated)
        .assert()
        .success()
        .stdout(contains("version 2"));

    rep(tmp.path())
        .args(["inspect", "cli_upd"])
        .assert()
        .success()
        .stdout(contains("round trip meta"));
}
