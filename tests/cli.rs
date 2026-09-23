use assert_cmd::Command;
use predicates::prelude::*;

fn rep(home: &tempfile::TempDir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_rep"));
    c.env("AGENT_REPERTOIRE_HOME", home.path());
    c
}

fn fixture_dir() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let unique = {
        let raw = format!(
            "smoke_cli_{}_{}",
            std::process::id(),
            dir.path().file_name().unwrap().to_str().unwrap()
        );
        let sanitized: String = raw
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.chars().next().map(|c| !c.is_ascii_lowercase()).unwrap_or(true) {
            format!("smoke_{}", sanitized)
        } else {
            sanitized
        }
    };
    std::fs::write(dir.path().join("run.sh"), "#!/usr/bin/env bash\nread -r input\necho \"{\\\"ok\\\": true, \\\"in\\\": $input}\"\n").unwrap();
    std::fs::write(
        dir.path().join("tool.yaml"),
        format!(
            "name: {name}\ndescription: CLI smoke tool {name}\nlanguage: bash\nentrypoint: run.sh\nkeywords:\n  - smoke\nexamples:\n  - x: demo\n",
            name = unique
        ),
    )
    .unwrap();
    (dir, unique)
}

#[test]
fn version_flag() {
    let home = tempfile::tempdir().unwrap();
    rep(&home)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn create_search_inspect_run_remove() {
    let home = tempfile::tempdir().unwrap();
    let (fixture, name) = fixture_dir();
    let yaml = fixture.path().join("tool.yaml");

    rep(&home)
        .args(["create", "--file", yaml.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("created smoke_cli_"));

    assert!(home
        .path()
        .join("tools")
        .join(&name)
        .join("run.sh")
        .exists());
    assert!(!home
        .path()
        .join("tools")
        .join(&name)
        .join("tool.yaml")
        .exists());

    rep(&home).args(["search", "smoke"]).assert().success().stdout(predicate::str::contains("smoke_cli_"));
    rep(&home).args(["list"]).assert().success().stdout(predicate::str::contains("smoke_cli_"));
    rep(&home)
        .args(["inspect", &name])
        .assert()
        .success()
        .stdout(predicate::str::contains("capabilities:"))
        .stdout(predicate::str::contains("examples:"))
        .stdout(predicate::str::contains("{\"x\":\"demo\"}"));
    rep(&home)
        .args(["run", &name, "x=hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));
    rep(&home)
        .args(["remove", &name, "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed"));
    rep(&home).args(["list"]).assert().success().stdout(predicate::str::contains("smoke_cli_").not());
}

#[test]
fn list_renders_multiline_entries() {
    let home = tempfile::tempdir().unwrap();
    let (fixture, name) = fixture_dir();
    let yaml = fixture.path().join("tool.yaml");
    rep(&home)
        .args(["create", "--file", yaml.to_str().unwrap()])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(
        &rep(&home)
            .args(["list"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone(),
    )
    .to_string();

    assert!(stdout.contains(&format!("{}\n", name)), "name on its own line");
    assert!(stdout.contains(" · "), "metadata segments joined by ·");
    assert!(stdout.contains("never used"));
    assert!(stdout.contains("keywords: smoke"));
    assert!(stdout.contains("    CLI smoke tool"), "indented description block");
    assert!(stdout.ends_with("1 tool\n"), "footer, singular");
}

#[test]
fn remove_without_yes_refuses_non_interactively() {
    let home = tempfile::tempdir().unwrap();
    rep(&home)
        .args(["remove", "whatever_tool"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to remove"));
}

#[test]
fn remove_piped_stdin_is_not_treated_as_confirmation() {
    let home = tempfile::tempdir().unwrap();
    let out = rep(&home)
        .args(["remove", "whatever_tool"])
        .write_stdin("yes\n")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("refusing to remove"));
}

#[test]
fn run_accepts_timeout_seconds() {
    let home = tempfile::tempdir().unwrap();
    let (fixture, name) = fixture_dir();
    let yaml = fixture.path().join("tool.yaml");
    rep(&home)
        .args(["create", "--file", yaml.to_str().unwrap()])
        .assert()
        .success();

    rep(&home)
        .args(["run", &name, "x=hello", "--timeout-seconds", "30"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));
}

#[test]
fn removed_subcommands_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    for cmd in ["doctor", "validate", "version"] {
        rep(&home)
            .args([cmd])
            .assert()
            .failure()
            .stderr(predicate::str::contains("unrecognized"));
    }
}

#[test]
fn create_rejects_removed_script_flag() {
    let home = tempfile::tempdir().unwrap();
    let (fixture, _name) = fixture_dir();
    let yaml = fixture.path().join("tool.yaml");
    rep(&home)
        .args([
            "create",
            "--file",
            yaml.to_str().unwrap(),
            "--script",
            fixture.path().join("run.sh").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}
