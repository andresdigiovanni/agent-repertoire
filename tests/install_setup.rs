use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;

#[test]
fn install_skill_configures_opencode_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--target", "opencode"])
        .assert()
        .success()
        .stdout(contains(
            "installed opencode\n  skill at ~/.config/opencode/skills/agent-repertoire\n  mcp server in ~/.config/opencode/opencode.json\n",
        ));
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(xdg.join("opencode/opencode.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(v["mcp"]["agent-repertoire"]["timeout"], 300000);
    assert!(xdg.join("opencode/skills/agent-repertoire/SKILL.md").is_file());
}

#[test]
fn install_skill_all_targets_reports_each() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--all"])
        .assert()
        .success()
        .stdout(contains("installed claude\n"))
        .stdout(contains("installed opencode\n"))
        .stdout(contains("installed codex\n"));
    assert!(tmp.path().join(".claude.json").is_file());
    assert!(tmp.path().join(".codex/config.toml").is_file());
    assert!(Path::new(&xdg.join("opencode/opencode.json")).exists());
}

#[test]
fn install_skill_all_continues_past_malformed_config() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    let oc_dir = xdg.join("opencode");
    std::fs::create_dir_all(&oc_dir).unwrap();
    std::fs::write(oc_dir.join("opencode.json"), "{not json").unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--all"])
        .assert()
        .success()
        .stderr(contains("warning: could not edit"));
    assert!(tmp.path().join(".claude.json").is_file());
    assert!(tmp.path().join(".codex/config.toml").is_file());
    assert_eq!(
        std::fs::read_to_string(oc_dir.join("opencode.json")).unwrap(),
        "{not json"
    );
}

#[test]
fn uninstall_removes_claude_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["install", "--target", "claude"])
        .assert()
        .success();
    assert!(tmp.path().join(".claude.json").is_file());

    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains(
            "uninstalled claude\n  skill at ~/.claude/skills/agent-repertoire\n  mcp server in ~/.claude.json\n",
        ));
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".claude.json")).unwrap(),
    )
    .unwrap();
    assert!(v.get("mcpServers").is_none_or(|s| s.get("agent-repertoire").is_none()));
}

#[test]
fn uninstall_removes_opencode_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--target", "opencode"])
        .assert()
        .success();
    let cfg = xdg.join("opencode/opencode.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert!(v["mcp"]["agent-repertoire"].is_object());

    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains(
            "uninstalled opencode\n  skill at ~/.config/opencode/skills/agent-repertoire\n  mcp server in ~/.config/opencode/opencode.json\n",
        ));
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert!(after.get("mcp").is_none_or(|m| m.get("agent-repertoire").is_none()));
    assert!(!xdg.join("opencode/skills/agent-repertoire").exists());
}

#[test]
fn uninstall_removes_codex_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["install", "--target", "codex"])
        .assert()
        .success();
    let cfg = tmp.path().join(".codex/config.toml");
    let before = std::fs::read_to_string(&cfg).unwrap();
    assert!(before.contains("agent-repertoire"));

    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains(
            "uninstalled codex\n  skill at ~/.codex/skills/agent-repertoire\n  mcp server in ~/.codex/config.toml\n",
        ));
    let after = std::fs::read_to_string(&cfg).unwrap();
    assert!(!after.contains("agent-repertoire"));
}

#[test]
fn uninstall_all_targets_strips_mcp_everywhere() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    let envs = || -> Command {
        let mut c = Command::cargo_bin("rep").unwrap();
        c.env("HOME", tmp.path()).env("XDG_CONFIG_HOME", &xdg);
        c
    };
    envs().args(["install", "--all"]).assert().success();

    let mut second = envs();
    second
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains(
            "uninstalled claude\n  skill at ~/.claude/skills/agent-repertoire\n  mcp server in ~/.claude.json\nuninstalled opencode\n  skill at ~/.config/opencode/skills/agent-repertoire\n  mcp server in ~/.config/opencode/opencode.json\nuninstalled codex\n  skill at ~/.codex/skills/agent-repertoire\n  mcp server in ~/.codex/config.toml\n",
        ));

    let claude: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".claude.json")).unwrap(),
    )
    .unwrap();
    assert!(claude
        .get("mcpServers")
        .is_none_or(|s| s.get("agent-repertoire").is_none()));

    let opencode: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(xdg.join("opencode/opencode.json")).unwrap(),
    )
    .unwrap();
    assert!(opencode
        .get("mcp")
        .is_none_or(|m| m.get("agent-repertoire").is_none()));

    let codex = std::fs::read_to_string(tmp.path().join(".codex/config.toml")).unwrap();
    assert!(!codex.contains("agent-repertoire"));
}

#[test]
fn uninstall_continues_past_malformed_config() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    let oc_dir = xdg.join("opencode");
    std::fs::create_dir_all(&oc_dir).unwrap();
    std::fs::write(oc_dir.join("opencode.json"), "{not json").unwrap();

    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--all"])
        .assert()
        .success();

    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains("uninstalled claude\n"))
        .stdout(contains("uninstalled codex\n"))
        .stderr(contains("warning: could not edit"));

    assert_eq!(
        std::fs::read_to_string(oc_dir.join("opencode.json")).unwrap(),
        "{not json"
    );

    let claude: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tmp.path().join(".claude.json")).unwrap(),
    )
    .unwrap();
    assert!(claude
        .get("mcpServers")
        .is_none_or(|s| s.get("agent-repertoire").is_none()));

    let codex = std::fs::read_to_string(tmp.path().join(".codex/config.toml")).unwrap();
    assert!(!codex.contains("agent-repertoire"));
}

#[test]
fn uninstall_repeated_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["install", "--target", "claude"])
        .assert()
        .success();
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains("uninstalled claude\n"));
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains("nothing to uninstall\n"));
}

#[test]
fn uninstall_yes_requires_all() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["uninstall", "--yes"])
        .assert()
        .failure()
        .stderr(contains("--yes"));
}

#[test]
fn uninstall_without_all_keeps_data_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::create_dir_all(tmp.path().join(".agent-repertoire")).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--all"])
        .assert()
        .success();
    // The data dir must exist after install.
    assert!(tmp.path().join(".agent-repertoire").is_dir());

    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["uninstall"])
        .assert()
        .success();

    // After plain `uninstall` (no --all), the data dir must still exist.
    assert!(
        tmp.path().join(".agent-repertoire").is_dir(),
        "data directory must not be deleted without --all"
    );
}

#[test]
fn uninstall_all_yes_deletes_data_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::create_dir_all(tmp.path().join(".agent-repertoire")).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--all"])
        .assert()
        .success();
    assert!(tmp.path().join(".agent-repertoire").is_dir());

    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["uninstall", "--all", "--yes"])
        .assert()
        .success()
        .stdout(contains("removed ~/.agent-repertoire\n"));

    assert!(
        !tmp.path().join(".agent-repertoire").exists(),
        "data directory must be deleted with --all --yes"
    );
}

#[test]
fn uninstall_all_without_yes_in_non_tty_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::create_dir_all(tmp.path().join(".agent-repertoire")).unwrap();
    let mut cmd = Command::cargo_bin("rep").unwrap();
    cmd.env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        // Redirect stdin from /dev/null — non-interactive.
        .write_stdin("/dev/null")
        .args(["uninstall", "--all"])
        .assert()
        .failure()
        .stderr(contains("pass --yes"));
}

#[test]
fn install_rerun_reports_already_configured_mcp() {
    let tmp = tempfile::tempdir().unwrap();
    let xdg = tmp.path().join(".config");
    std::fs::create_dir_all(&xdg).unwrap();
    let mut first = Command::cargo_bin("rep").unwrap();
    first
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--target", "opencode"])
        .assert()
        .success();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env("XDG_CONFIG_HOME", &xdg)
        .args(["install", "--target", "opencode"])
        .assert()
        .success()
        .stdout(contains(
            "installed opencode\n  skill at ~/.config/opencode/skills/agent-repertoire\n  mcp server already configured in ~/.config/opencode/opencode.json\n",
        ));
}

#[test]
fn uninstall_clean_home_reports_nothing_to_uninstall() {
    let tmp = tempfile::tempdir().unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains("nothing to uninstall\n"));
}

#[test]
fn uninstall_reports_only_pieces_that_existed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut setup = Command::cargo_bin("rep").unwrap();
    setup
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["install", "--target", "claude"])
        .assert()
        .success();
    // Remove the skill dir by hand, leaving only the MCP entry.
    std::fs::remove_dir_all(tmp.path().join(".claude/skills/agent-repertoire")).unwrap();
    Command::cargo_bin("rep")
        .unwrap()
        .env("HOME", tmp.path())
        .env_remove("XDG_CONFIG_HOME")
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(contains(
            "uninstalled claude\n  mcp server in ~/.claude.json\n",
        ));
}
