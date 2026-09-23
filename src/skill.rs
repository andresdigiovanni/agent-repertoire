use crate::{RepError, Result};
use std::path::PathBuf;

pub const SKILL_MD: &str = include_str!("../skill/SKILL.md");
pub const REFERENCES_CREATING_TOOLS_MD: &str = include_str!("../skill/references/creating-tools.md");

const SKILL_DIR_NAME: &str = "agent-repertoire";
const SKILL_FILE_NAME: &str = "SKILL.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SkillTarget {
    Claude,
    Opencode,
    Codex,
}

impl SkillTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            SkillTarget::Claude => "claude",
            SkillTarget::Opencode => "opencode",
            SkillTarget::Codex => "codex",
        }
    }

    pub fn all() -> &'static [SkillTarget] {
        &[SkillTarget::Claude, SkillTarget::Opencode, SkillTarget::Codex]
    }
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| RepError::Spawn("cannot determine home directory; set HOME".into()))
}

pub fn target_dir(target: SkillTarget) -> Result<PathBuf> {
    let home = home_dir()?;
    let base = match target {
        SkillTarget::Claude => home.join(".claude"),
        SkillTarget::Opencode => {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| home.join(".config"));
            config.join("opencode")
        }
        SkillTarget::Codex => home.join(".codex"),
    };
    Ok(base.join("skills").join(SKILL_DIR_NAME))
}

pub fn target_path(target: SkillTarget) -> Result<PathBuf> {
    Ok(target_dir(target)?.join(SKILL_FILE_NAME))
}

pub fn install_skill(target: SkillTarget) -> Result<PathBuf> {
    let dir = target_dir(target)?;
    if dir.exists() {
        tracing::warn!(path = %dir.display(), "overwriting existing skill directory");
    }
    std::fs::create_dir_all(dir.join("references"))?;
    std::fs::write(dir.join(SKILL_FILE_NAME), SKILL_MD)?;
    std::fs::write(dir.join("references").join("creating-tools.md"), REFERENCES_CREATING_TOOLS_MD)?;
    Ok(dir.join(SKILL_FILE_NAME))
}

pub fn uninstall_skill(target: SkillTarget) -> Result<()> {
    let dir = target_dir(target)?;
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ENV_LOCK;

    fn skill_body() -> &'static str {
        SKILL_MD
    }

    #[test]
    fn embedded_skill_has_expected_sections() {
        let body = skill_body();
        assert!(body.contains("name: agent-repertoire"));
        assert!(body.contains("# Agent Repertoire"));
        assert!(body.contains("references/creating-tools.md"));
        assert!(REFERENCES_CREATING_TOOLS_MD.contains("#"));
    }

    #[test]
    fn claude_target_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HOME", "/tmp/fake-home-claude");
        std::env::remove_var("XDG_CONFIG_HOME");
        let path = target_path(SkillTarget::Claude).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/fake-home-claude/.claude/skills/agent-repertoire/SKILL.md")
        );
        std::env::remove_var("HOME");
    }

    #[test]
    fn opencode_target_path_prefers_xdg() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HOME", "/tmp/fake-home-open");
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/fake-xdg");
        let path = target_path(SkillTarget::Opencode).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/fake-xdg/opencode/skills/agent-repertoire/SKILL.md")
        );
        std::env::remove_var("XDG_CONFIG_HOME");
        let path = target_path(SkillTarget::Opencode).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/fake-home-open/.config/opencode/skills/agent-repertoire/SKILL.md")
        );
        std::env::remove_var("HOME");
    }

    #[test]
    fn codex_target_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HOME", "/tmp/fake-home-codex");
        std::env::remove_var("XDG_CONFIG_HOME");
        let path = target_path(SkillTarget::Codex).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/fake-home-codex/.codex/skills/agent-repertoire/SKILL.md")
        );
        std::env::remove_var("HOME");
    }

    #[test]
    fn missing_home_is_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("HOME");
        std::env::remove_var("XDG_CONFIG_HOME");
        let err = target_path(SkillTarget::Claude).unwrap_err();
        assert!(err.to_string().contains("cannot determine home directory"));
        std::env::set_var("HOME", "/tmp/fake-home-restore");
    }

    #[test]
    fn empty_home_is_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("HOME", "");
        let err = target_path(SkillTarget::Claude).unwrap_err();
        assert!(err.to_string().contains("cannot determine home directory"));
        std::env::set_var("HOME", "/tmp/fake-home-restore");
    }

    #[test]
    fn install_writes_skill_and_is_idempotent() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path().to_str().unwrap());
        std::env::remove_var("XDG_CONFIG_HOME");

        let path = install_skill(SkillTarget::Claude).unwrap();
        let expected = dir.path().join(".claude/skills/agent-repertoire/SKILL.md");
        assert_eq!(path, expected);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL_MD);

        let again = install_skill(SkillTarget::Claude).unwrap();
        assert_eq!(again, path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SKILL_MD);

        let reference = dir.path().join(".claude/skills/agent-repertoire/references/creating-tools.md");
        assert_eq!(std::fs::read_to_string(&reference).unwrap(), REFERENCES_CREATING_TOOLS_MD);
        let again_dir = dir.path().join(".claude/skills/agent-repertoire");
        assert!(again_dir.join("SKILL.md").exists());

        std::env::remove_var("HOME");
    }

    #[test]
    fn uninstall_removes_and_noops_when_absent() {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path().to_str().unwrap());
        std::env::remove_var("XDG_CONFIG_HOME");

        install_skill(SkillTarget::Codex).unwrap();
        let path = target_path(SkillTarget::Codex).unwrap();
        assert!(path.exists());
        uninstall_skill(SkillTarget::Codex).unwrap();
        let dir = target_dir(SkillTarget::Codex).unwrap();
        assert!(!dir.exists());
        uninstall_skill(SkillTarget::Codex).unwrap();

        std::env::remove_var("HOME");
    }
}
