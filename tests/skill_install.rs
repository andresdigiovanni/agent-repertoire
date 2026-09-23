use assert_cmd::Command;
use predicates::prelude::*;

fn rep() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rep"))
}

#[test]
fn install_without_target_non_tty_fails() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    rep()
        .args(["install"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no provider selected; pass --target <claude|opencode|codex> or --all",
        ));
    assert!(!home.path().join(".claude/skills/agent-repertoire/SKILL.md").exists());
    assert!(!home.path().join(".claude.json").exists());
}

#[test]
fn install_target_claude_creates_claude_skill() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    rep()
        .args(["install", "--target", "claude"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed claude\n  skill at ~/.claude/skills/agent-repertoire\n  mcp server in ~/.claude.json\n",
        ));
    let installed = home.path().join(".claude/skills/agent-repertoire/SKILL.md");
    assert_eq!(
        std::fs::read_to_string(&installed).unwrap(),
        agent_repertoire::skill::SKILL_MD
    );
    assert_eq!(
        std::fs::read_to_string(home.path().join(".claude/skills/agent-repertoire/references/creating-tools.md")).unwrap(),
        agent_repertoire::skill::REFERENCES_CREATING_TOOLS_MD
    );
}

#[test]
fn install_all_creates_every_target() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    rep()
        .args(["install", "--all"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("installed claude\n"))
        .stdout(predicate::str::contains("installed opencode\n"))
        .stdout(predicate::str::contains("installed codex\n"));
    assert!(home.path().join(".claude/skills/agent-repertoire/SKILL.md").exists());
    assert!(xdg.path().join("opencode/skills/agent-repertoire/SKILL.md").exists());
    assert!(home.path().join(".codex/skills/agent-repertoire/SKILL.md").exists());
    assert!(home.path().join(".claude/skills/agent-repertoire/references/creating-tools.md").exists());
    assert!(xdg.path().join("opencode/skills/agent-repertoire/references/creating-tools.md").exists());
    assert!(home.path().join(".codex/skills/agent-repertoire/references/creating-tools.md").exists());
}

#[test]
fn install_explicit_target_opencode() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    rep()
        .args(["install", "--target", "opencode"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("installed opencode\n"));
    assert!(xdg.path().join("opencode/skills/agent-repertoire/SKILL.md").exists());
    assert!(!home.path().join(".claude/skills/agent-repertoire/SKILL.md").exists());
}

#[test]
fn target_and_all_conflict() {
    let home = tempfile::tempdir().unwrap();
    rep()
        .args(["install", "--target", "claude", "--all"])
        .env("HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn unknown_target_rejected() {
    let home = tempfile::tempdir().unwrap();
    rep()
        .args(["install", "--target", "nope"])
        .env("HOME", home.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn uninstall_removes_every_target() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let mut c = rep();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", xdg.path());
    c.args(["install", "--all"]).assert().success();
    assert!(home.path().join(".claude/skills/agent-repertoire/SKILL.md").exists());
    assert!(xdg.path().join("opencode/skills/agent-repertoire/SKILL.md").exists());
    assert!(home.path().join(".codex/skills/agent-repertoire/SKILL.md").exists());

    let mut c = rep();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", xdg.path());
    let oc_block = format!(
        "uninstalled opencode\n  skill at {}\n  mcp server in {}\n",
        xdg.path().join("opencode/skills/agent-repertoire").display(),
        xdg.path().join("opencode/opencode.json").display()
    );
    c.args(["uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "uninstalled claude\n  skill at ~/.claude/skills/agent-repertoire\n  mcp server in ~/.claude.json\n",
        ))
        .stdout(predicate::str::contains(oc_block.as_str()))
        .stdout(predicate::str::contains(
            "uninstalled codex\n  skill at ~/.codex/skills/agent-repertoire\n  mcp server in ~/.codex/config.toml\n",
        ));
    assert!(!home.path().join(".claude/skills/agent-repertoire").exists());
    assert!(!xdg.path().join("opencode/skills/agent-repertoire").exists());
    assert!(!home.path().join(".codex/skills/agent-repertoire").exists());

    let claude_v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
    )
    .unwrap();
    assert!(claude_v
        .get("mcpServers")
        .is_none_or(|s| s.get("agent-repertoire").is_none()));
    let opencode_v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(xdg.path().join("opencode/opencode.json")).unwrap(),
    )
    .unwrap();
    assert!(opencode_v
        .get("mcp")
        .is_none_or(|m| m.get("agent-repertoire").is_none()));
    let codex_text =
        std::fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
    assert!(!codex_text.contains("agent-repertoire"));
}

#[test]
fn uninstall_removes_then_reports_nothing_to_uninstall() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    let setup = || {
        let mut c = rep();
        c.env("HOME", home.path()).env("XDG_CONFIG_HOME", xdg.path());
        c
    };
    setup().args(["install", "--target", "codex"]).assert().success();
    setup()
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains("uninstalled codex\n"));
    assert!(!home.path().join(".codex/skills/agent-repertoire/SKILL.md").exists());
    setup()
        .args(["uninstall"])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to uninstall\n"));
}
