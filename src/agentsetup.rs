use crate::skill::SkillTarget;
use crate::{RepError, Result};
use std::path::{Path, PathBuf};

pub const MCP_TIMEOUT_MS: u64 = 300000;

#[derive(Debug, Clone, PartialEq)]
pub enum SetupAction {
    Created,
    Patched,
    AlreadyConfigured,
    NeedsManualEdit(String),
    Removed,
    NotPresent,
}

#[derive(Debug, Clone)]
pub struct SetupOutcome {
    pub target: SkillTarget,
    pub config_path: PathBuf,
    pub action: SetupAction,
}

impl SetupAction {
    fn map_hint(self, extra: String) -> Self {
        match self {
            SetupAction::NeedsManualEdit(hint) => {
                SetupAction::NeedsManualEdit(format!("{hint} {extra}"))
            }
            other => other,
        }
    }
}

pub(crate) fn strip_jsonc(raw: &str) -> (String, bool) {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut had_comments = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                had_comments = true;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < chars.len() && chars[i + 1] == '*' => {
                had_comments = true;
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                out.push(' ');
            }
            ',' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                    i = j;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, had_comments)
}

fn opencode_dir(home: &Path) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".config"));
    base.join("opencode")
}

fn opencode_entry(exe: &Path) -> serde_json::Value {
    serde_json::json!({
        "type": "local",
        "command": [exe.display().to_string(), "mcp"],
        "enabled": true,
        "timeout": MCP_TIMEOUT_MS,
    })
}

fn manual_hint(entry: &serde_json::Value) -> SetupAction {
    SetupAction::NeedsManualEdit(format!(
        "\"agent-repertoire\": {}",
        serde_json::to_string_pretty(entry).unwrap_or_default()
    ))
}

fn configure_opencode(home: &Path, exe: &Path) -> Result<SetupOutcome> {
    let dir = opencode_dir(home);
    std::fs::create_dir_all(&dir)?;
    let jsonc = dir.join("opencode.jsonc");
    let json = dir.join("opencode.json");
    let entry = opencode_entry(exe);
    let (path, raw) = if jsonc.is_file() {
        let raw = std::fs::read_to_string(&jsonc)?;
        (jsonc, Some(raw))
    } else if json.is_file() {
        let raw = std::fs::read_to_string(&json)?;
        (json, Some(raw))
    } else {
        let mut root = serde_json::Map::new();
        root.insert(
            "mcp".into(),
            serde_json::json!({ "agent-repertoire": entry }),
        );
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(root))?;
        std::fs::write(&json, text)?;
        return Ok(SetupOutcome {
            target: SkillTarget::Opencode,
            config_path: json,
            action: SetupAction::Created,
        });
    };

    let (clean, had_comments) = strip_jsonc(&raw.unwrap());
    if had_comments {
        return Ok(SetupOutcome {
            target: SkillTarget::Opencode,
            config_path: path,
            action: manual_hint(&entry),
        });
    }
    let mut root: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(e) => {
            return Ok(SetupOutcome {
                target: SkillTarget::Opencode,
                config_path: path,
                action: manual_hint(&entry).map_hint(format!("(config is not valid JSON: {})", e)),
            })
        }
    };
    let obj = match root.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                action: manual_hint(&entry),
                config_path: path,
                target: SkillTarget::Opencode,
            })
        }
    };
    let mcp = obj.entry("mcp").or_insert(serde_json::json!({}));
    let mcp_obj = match mcp.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                action: manual_hint(&entry),
                config_path: path,
                target: SkillTarget::Opencode,
            })
        }
    };
    let existed = mcp_obj.contains_key("agent-repertoire");
    let existing = mcp_obj
        .entry("agent-repertoire")
        .or_insert(serde_json::json!({}));
    let existing_obj = match existing.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                action: manual_hint(&entry),
                config_path: path,
                target: SkillTarget::Opencode,
            })
        }
    };
    let mut changed = false;
    if !existed {
        *existing_obj = entry.as_object().cloned().unwrap_or_default();
        changed = true;
    } else {
        for (k, v) in entry.as_object().unwrap() {
            if k == "timeout" {
                let current = existing_obj.get("timeout").and_then(|t| t.as_u64());
                if current.is_none() || current.unwrap() < MCP_TIMEOUT_MS {
                    existing_obj.insert(k.clone(), v.clone());
                    changed = true;
                }
            } else if k == "command" {
                let is_array = existing_obj
                    .get("command")
                    .map(|c| c.is_array())
                    .unwrap_or(false);
                if !is_array {
                    existing_obj.insert(k.clone(), v.clone());
                    changed = true;
                }
            } else if !existing_obj.contains_key(k) {
                existing_obj.insert(k.clone(), v.clone());
                changed = true;
            }
        }
    }
    let action = if !existed {
        SetupAction::Created
    } else if changed {
        SetupAction::Patched
    } else {
        SetupAction::AlreadyConfigured
    };
    if changed {
        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(SetupOutcome {
        target: SkillTarget::Opencode,
        config_path: path,
        action,
    })
}

fn claude_entry(exe: &Path) -> serde_json::Value {
    serde_json::json!({
        "command": exe.display().to_string(),
        "args": ["mcp"],
        "env": {
            "MCP_TIMEOUT": MCP_TIMEOUT_MS.to_string(),
            "MCP_TOOL_TIMEOUT": MCP_TIMEOUT_MS.to_string(),
        }
    })
}

fn configure_claude(home: &Path, exe: &Path) -> Result<SetupOutcome> {
    let path = home.join(".claude.json");
    let file_existed = path.is_file();
    let entry = claude_entry(exe);
    let mut root: serde_json::Value = if path.is_file() {
        let raw = std::fs::read_to_string(&path)?;
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                return Ok(SetupOutcome {
                    target: SkillTarget::Claude,
                    config_path: path,
                    action: manual_hint(&entry)
                        .map_hint(format!("(file is not valid JSON: {})", e)),
                })
            }
        }
    } else {
        serde_json::json!({})
    };
    let obj = match root.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Claude,
                config_path: path,
                action: manual_hint(&entry),
            })
        }
    };
    let servers = obj.entry("mcpServers").or_insert(serde_json::json!({}));
    let servers_obj = match servers.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Claude,
                config_path: path,
                action: manual_hint(&entry),
            })
        }
    };
    let existed = servers_obj.contains_key("agent-repertoire");
    let slot = servers_obj
        .entry("agent-repertoire")
        .or_insert(serde_json::json!({}));
    let slot_obj = match slot.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Claude,
                config_path: path,
                action: manual_hint(&entry),
            })
        }
    };
    let mut changed = false;
    if !existed {
        *slot_obj = entry.as_object().cloned().unwrap_or_default();
        changed = true;
    } else {
        if !slot_obj.contains_key("command") {
            slot_obj.insert(
                "command".into(),
                serde_json::json!(exe.display().to_string()),
            );
            changed = true;
        }
        if !slot_obj.contains_key("args") {
            slot_obj.insert("args".into(), serde_json::json!(["mcp"]));
            changed = true;
        }
        let env = slot_obj.entry("env").or_insert(serde_json::json!({}));
        if let Some(env_obj) = env.as_object_mut() {
            for (k, v) in entry["env"].as_object().unwrap() {
                if env_obj.get(k) != Some(v) {
                    env_obj.insert(k.clone(), v.clone());
                    changed = true;
                }
            }
        }
    }
    let action = if !file_existed {
        SetupAction::Created
    } else if changed {
        SetupAction::Patched
    } else {
        SetupAction::AlreadyConfigured
    };
    if changed {
        std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    }
    Ok(SetupOutcome {
        target: SkillTarget::Claude,
        config_path: path,
        action,
    })
}

fn codex_startup_timeout() -> u64 {
    MCP_TIMEOUT_MS / 1000
}

fn configure_codex(home: &Path, exe: &Path) -> Result<SetupOutcome> {
    let dir = home.join(".codex");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let raw = if path.is_file() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let mut doc = match raw.parse::<toml_edit::DocumentMut>() {
        Ok(doc) => doc,
        Err(e) => {
            return Ok(SetupOutcome {
                target: SkillTarget::Codex,
                config_path: path,
                action: SetupAction::NeedsManualEdit(format!(
                    "config is not valid TOML: {}\nadd manually:\n[mcp_servers.agent-repertoire]\ncommand = \"{}\"\nargs = [\"mcp\"]\nstartup_timeout_sec = {}\ntool_timeout_sec = {}",
                    e,
                    exe.display(),
                    codex_startup_timeout(),
                    codex_startup_timeout()
                )),
            })
        }
    };
    if doc.get("mcp_servers").is_none() {
        doc.insert(
            "mcp_servers",
            toml_edit::Item::Table(toml_edit::Table::new()),
        );
    }
    let servers = doc["mcp_servers"]
        .as_table_mut()
        .ok_or_else(|| RepError::Spawn("codex config: mcp_servers is not a table".into()))?;
    servers.set_implicit(true);
    let existed = servers.contains_key("agent-repertoire");
    if !existed {
        servers.insert(
            "agent-repertoire",
            toml_edit::Item::Table(toml_edit::Table::new()),
        );
    }
    let entry = servers["agent-repertoire"].as_table_mut().ok_or_else(|| {
        RepError::Spawn("codex config: mcp_servers.agent-repertoire is not a table".into())
    })?;
    let mut changed = false;
    if entry.get("command").is_none() {
        entry.insert("command", toml_edit::value(exe.display().to_string()));
        changed = true;
    }
    if entry.get("args").is_none() {
        let mut args = toml_edit::Array::new();
        args.push("mcp");
        entry.insert("args", toml_edit::value(args));
        changed = true;
    }
    if entry.get("startup_timeout_sec").is_none() {
        entry.insert(
            "startup_timeout_sec",
            toml_edit::value(codex_startup_timeout() as i64),
        );
        changed = true;
    }
    if entry.get("tool_timeout_sec").is_none() {
        entry.insert(
            "tool_timeout_sec",
            toml_edit::value(codex_startup_timeout() as i64),
        );
        changed = true;
    }
    let action = if !existed {
        SetupAction::Created
    } else if changed {
        SetupAction::Patched
    } else {
        SetupAction::AlreadyConfigured
    };
    if changed {
        std::fs::write(&path, doc.to_string())?;
    }
    Ok(SetupOutcome {
        target: SkillTarget::Codex,
        config_path: path,
        action,
    })
}

pub fn configure_mcp_for(target: SkillTarget, home: &Path, exe: &Path) -> Result<SetupOutcome> {
    match target {
        SkillTarget::Opencode => configure_opencode(home, exe),
        SkillTarget::Claude => configure_claude(home, exe),
        SkillTarget::Codex => configure_codex(home, exe),
    }
}

fn deconfigure_opencode(home: &Path) -> Result<SetupOutcome> {
    let dir = opencode_dir(home);
    let jsonc = dir.join("opencode.jsonc");
    let json = dir.join("opencode.json");
    let (path, raw) = if jsonc.is_file() {
        let raw = std::fs::read_to_string(&jsonc)?;
        (jsonc, raw)
    } else if json.is_file() {
        let raw = std::fs::read_to_string(&json)?;
        (json, raw)
    } else {
        return Ok(SetupOutcome {
            target: SkillTarget::Opencode,
            config_path: json,
            action: SetupAction::NotPresent,
        });
    };
    let (clean, had_comments) = strip_jsonc(&raw);
    if had_comments {
        return Ok(SetupOutcome {
            target: SkillTarget::Opencode,
            config_path: path,
            action: SetupAction::NeedsManualEdit(
                "opencode config contains comments; remove the agent-repertoire block by hand"
                    .to_string(),
            ),
        });
    }
    let mut root: serde_json::Value = match serde_json::from_str(&clean) {
        Ok(v) => v,
        Err(e) => {
            return Ok(SetupOutcome {
                target: SkillTarget::Opencode,
                config_path: path,
                action: SetupAction::NeedsManualEdit(format!(
                    "config is not valid JSON: {}",
                    e
                )),
            })
        }
    };
    let obj = match root.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Opencode,
                config_path: path,
                action: SetupAction::NotPresent,
            })
        }
    };
    let removed = match obj.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        Some(mcp_obj) => mcp_obj.remove("agent-repertoire").is_some(),
        None => false,
    };
    if !removed {
        return Ok(SetupOutcome {
            target: SkillTarget::Opencode,
            config_path: path,
            action: SetupAction::NotPresent,
        });
    }
    if let Some(mcp) = obj.get("mcp") {
        if mcp.as_object().map(|o| o.is_empty()).unwrap_or(false) {
            obj.remove("mcp");
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(SetupOutcome {
        target: SkillTarget::Opencode,
        config_path: path,
        action: SetupAction::Removed,
    })
}

pub fn deconfigure_mcp_for(target: SkillTarget, home: &Path) -> Result<SetupOutcome> {
    match target {
        SkillTarget::Opencode => deconfigure_opencode(home),
        SkillTarget::Claude => deconfigure_claude(home),
        SkillTarget::Codex => deconfigure_codex(home),
    }
}

fn deconfigure_codex(home: &Path) -> Result<SetupOutcome> {
    let dir = home.join(".codex");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    if !path.is_file() {
        return Ok(SetupOutcome {
            target: SkillTarget::Codex,
            config_path: path,
            action: SetupAction::NotPresent,
        });
    }
    let raw = std::fs::read_to_string(&path)?;
    let mut doc = match raw.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(e) => {
            return Ok(SetupOutcome {
                target: SkillTarget::Codex,
                config_path: path,
                action: SetupAction::NeedsManualEdit(format!(
                    "config is not valid TOML: {}",
                    e
                )),
            })
        }
    };
    let servers = match doc.get_mut("mcp_servers").and_then(|i| i.as_table_mut()) {
        Some(t) => t,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Codex,
                config_path: path,
                action: SetupAction::NotPresent,
            })
        }
    };
    if servers.remove("agent-repertoire").is_none() {
        return Ok(SetupOutcome {
            target: SkillTarget::Codex,
            config_path: path,
            action: SetupAction::NotPresent,
        });
    }
    std::fs::write(&path, doc.to_string())?;
    Ok(SetupOutcome {
        target: SkillTarget::Codex,
        config_path: path,
        action: SetupAction::Removed,
    })
}

fn deconfigure_claude(home: &Path) -> Result<SetupOutcome> {
    let path = home.join(".claude.json");
    if !path.is_file() {
        return Ok(SetupOutcome {
            target: SkillTarget::Claude,
            config_path: path,
            action: SetupAction::NotPresent,
        });
    }
    let raw = std::fs::read_to_string(&path)?;
    let mut root: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return Ok(SetupOutcome {
                target: SkillTarget::Claude,
                config_path: path,
                action: SetupAction::NeedsManualEdit(format!(
                    "config is not valid JSON: {}",
                    e
                )),
            })
        }
    };
    let obj = match root.as_object_mut() {
        Some(o) => o,
        None => {
            return Ok(SetupOutcome {
                target: SkillTarget::Claude,
                config_path: path,
                action: SetupAction::NotPresent,
            })
        }
    };
    let removed = match obj
        .get_mut("mcpServers")
        .and_then(|v| v.as_object_mut())
    {
        Some(servers) => servers.remove("agent-repertoire").is_some(),
        None => false,
    };
    if !removed {
        return Ok(SetupOutcome {
            target: SkillTarget::Claude,
            config_path: path,
            action: SetupAction::NotPresent,
        });
    }
    if let Some(servers) = obj.get("mcpServers") {
        if servers.as_object().map(|o| o.is_empty()).unwrap_or(false) {
            obj.remove("mcpServers");
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root)?)?;
    Ok(SetupOutcome {
        target: SkillTarget::Claude,
        config_path: path,
        action: SetupAction::Removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ENV_LOCK;

    struct EnvGuard {
        keys: Vec<&'static str>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn clear(keys: &'static [&'static str]) -> Self {
            let lock = ENV_LOCK.lock().unwrap();
            for k in keys {
                std::env::remove_var(k);
            }
            Self {
                keys: keys.to_vec(),
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for k in &self.keys {
                std::env::remove_var(k);
            }
        }
    }

    #[test]
    fn strips_comments_and_trailing_commas() {
        let (clean, had) =
            strip_jsonc("{\n  // line\n  \"a\": 1, /* block */\n  \"b\": [1, 2,],\n}");
        assert!(had);
        let v: serde_json::Value = serde_json::from_str(&clean).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], serde_json::json!([1, 2]));
    }

    #[test]
    fn urls_in_strings_are_not_comments() {
        let (clean, had) = strip_jsonc(r#"{"url": "https://x.io//y", "z": 1}"#);
        assert!(!had);
        let v: serde_json::Value = serde_json::from_str(&clean).unwrap();
        assert_eq!(v["url"], "https://x.io//y");
    }

    #[test]
    fn plain_json_has_no_comments() {
        let (clean, had) = strip_jsonc("{\"a\": {\"b\": 2}}");
        assert!(!had);
        assert_eq!(clean, "{\"a\": {\"b\": 2}}");
    }

    #[test]
    fn opencode_creates_config_when_missing() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let exe = Path::new("/usr/local/bin/rep");
        let out = configure_mcp_for(SkillTarget::Opencode, home.path(), exe).unwrap();
        assert_eq!(out.action, SetupAction::Created);
        let cfg = home.path().join(".config/opencode/opencode.json");
        assert_eq!(out.config_path, cfg);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["mcp"]["agent-repertoire"]["timeout"], 300000);
        assert_eq!(
            v["mcp"]["agent-repertoire"]["command"],
            serde_json::json!(["/usr/local/bin/rep", "mcp"])
        );
        assert_eq!(v["mcp"]["agent-repertoire"]["enabled"], true);
    }

    #[test]
    fn opencode_patches_low_timeout_and_is_idempotent() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.json"),
            r#"{"mcp": {"agent-repertoire": {"type": "local", "command": ["/old/rep", "mcp"], "enabled": true, "timeout": 5000}}, "other": 1}"#,
        )
        .unwrap();
        let exe = Path::new("/new/rep");
        let out = configure_mcp_for(SkillTarget::Opencode, home.path(), exe).unwrap();
        assert_eq!(out.action, SetupAction::Patched);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap())
                .unwrap();
        assert_eq!(v["mcp"]["agent-repertoire"]["timeout"], 300000);
        assert_eq!(
            v["mcp"]["agent-repertoire"]["command"],
            serde_json::json!(["/old/rep", "mcp"])
        );
        assert_eq!(v["other"], 1);

        let again = configure_mcp_for(SkillTarget::Opencode, home.path(), exe).unwrap();
        assert_eq!(again.action, SetupAction::AlreadyConfigured);
    }

    #[test]
    fn opencode_jsonc_with_comments_needs_manual_edit() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.jsonc"),
            "{\n  // my settings\n  \"mcp\": {}\n}",
        )
        .unwrap();
        let out =
            configure_mcp_for(SkillTarget::Opencode, home.path(), Path::new("/bin/rep")).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(_) => {}
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        let after = std::fs::read_to_string(cfg_dir.join("opencode.jsonc")).unwrap();
        assert!(after.contains("// my settings"));
    }

    #[test]
    fn claude_creates_and_preserves_existing_entries() {
        let home = tempfile::tempdir().unwrap();
        let claude_json = home.path().join(".claude.json");
        std::fs::write(
            &claude_json,
            r#"{"mcpServers": {"other-server": {"command": "/bin/x"}}, "numStartups": 3}"#,
        )
        .unwrap();
        let out =
            configure_mcp_for(SkillTarget::Claude, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(out.action, SetupAction::Patched);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&claude_json).unwrap()).unwrap();
        let entry = &v["mcpServers"]["agent-repertoire"];
        assert_eq!(entry["command"], "/bin/rep");
        assert_eq!(entry["args"], serde_json::json!(["mcp"]));
        assert_eq!(entry["env"]["MCP_TOOL_TIMEOUT"], "300000");
        assert_eq!(v["mcpServers"]["other-server"]["command"], "/bin/x");
        assert_eq!(v["numStartups"], 3);

        let again =
            configure_mcp_for(SkillTarget::Claude, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(again.action, SetupAction::AlreadyConfigured);
    }

    #[test]
    fn claude_creates_file_when_missing() {
        let home = tempfile::tempdir().unwrap();
        let out =
            configure_mcp_for(SkillTarget::Claude, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(out.action, SetupAction::Created);
        let v: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(home.path().join(".claude.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            v["mcpServers"]["agent-repertoire"]["env"]["MCP_TIMEOUT"],
            "300000"
        );
    }

    #[test]
    fn codex_creates_and_patches_toml() {
        let home = tempfile::tempdir().unwrap();
        let out =
            configure_mcp_for(SkillTarget::Codex, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(out.action, SetupAction::Created);
        let toml_text = std::fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
        assert!(toml_text.contains("[mcp_servers.agent-repertoire]"));
        assert!(toml_text.contains("command = \"/bin/rep\""));
        assert!(toml_text.contains("args = [\"mcp\"]"));

        std::fs::write(
            home.path().join(".codex/config.toml"),
            "# my comment\nother_key = 1\n",
        )
        .unwrap();
        let out2 =
            configure_mcp_for(SkillTarget::Codex, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(out2.action, SetupAction::Created);
        let after = std::fs::read_to_string(home.path().join(".codex/config.toml")).unwrap();
        assert!(after.contains("# my comment"));
        assert!(after.contains("startup_timeout_sec"));
    }

    #[test]
    fn opencode_repairs_string_command_entry() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.json"),
            r#"{"mcp": {"agent-repertoire": {"type": "local", "command": "/old/rep", "enabled": true, "timeout": 300000}}}"#,
        )
        .unwrap();
        let exe = Path::new("/new/rep");
        let out = configure_mcp_for(SkillTarget::Opencode, home.path(), exe).unwrap();
        assert_eq!(out.action, SetupAction::Patched);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap())
                .unwrap();
        assert_eq!(
            v["mcp"]["agent-repertoire"]["command"],
            serde_json::json!(["/new/rep", "mcp"])
        );
        assert_eq!(v["mcp"]["agent-repertoire"]["timeout"], 300000);
    }

    #[test]
    fn opencode_malformed_json_needs_manual_edit() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("opencode.json"), "{not json").unwrap();
        let out =
            configure_mcp_for(SkillTarget::Opencode, home.path(), Path::new("/bin/rep")).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(hint) => assert!(hint.contains("not valid JSON")),
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        let after = std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap();
        assert_eq!(after, "{not json");
    }

    #[test]
    fn codex_malformed_toml_needs_manual_edit() {
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".codex");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("config.toml"), "[[[broken").unwrap();
        let out =
            configure_mcp_for(SkillTarget::Codex, home.path(), Path::new("/bin/rep")).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(hint) => assert!(hint.contains("not valid TOML")),
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        let after = std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap();
        assert_eq!(after, "[[[broken");
    }

    #[test]
    fn opencode_fills_missing_fields_of_partial_entry() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.json"),
            r#"{"mcp": {"agent-repertoire": {"command": ["/keep/rep", "mcp"]}}}"#,
        )
        .unwrap();
        let out =
            configure_mcp_for(SkillTarget::Opencode, home.path(), Path::new("/bin/rep")).unwrap();
        assert_eq!(out.action, SetupAction::Patched);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap())
                .unwrap();
        assert_eq!(
            v["mcp"]["agent-repertoire"]["command"],
            serde_json::json!(["/keep/rep", "mcp"])
        );
        assert_eq!(v["mcp"]["agent-repertoire"]["timeout"], 300000);
        assert_eq!(v["mcp"]["agent-repertoire"]["enabled"], true);
    }

    #[test]
    fn opencode_removes_existing_entry_and_keeps_other_mcps() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.json"),
            r#"{"mcp": {"agent-repertoire": {"type": "local", "command": ["/bin/rep", "mcp"], "enabled": true}, "other": {"type": "local", "command": ["x", "mcp"]}}, "theme": "dark"}"#,
        )
        .unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap())
                .unwrap();
        assert!(v["mcp"].get("agent-repertoire").is_none());
        assert_eq!(v["mcp"]["other"]["command"], serde_json::json!(["x", "mcp"]));
        assert_eq!(v["theme"], "dark");
    }

    #[test]
    fn opencode_no_entry_is_not_present() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let original = r#"{"mcp": {"other": {"command": ["x"]}}, "theme": "dark"}"#;
        std::fs::write(cfg_dir.join("opencode.json"), original).unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap(),
            original
        );
    }

    #[test]
    fn opencode_missing_file_is_not_present() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
    }

    #[test]
    fn opencode_deconfigure_jsonc_with_comments_needs_manual_edit() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let original = "{\n  // my settings\n  \"mcp\": {\"agent-repertoire\": {}}\n}";
        std::fs::write(cfg_dir.join("opencode.jsonc"), original).unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(_) => {}
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("opencode.jsonc")).unwrap(),
            original
        );
    }

    #[test]
    fn opencode_deconfigure_malformed_json_needs_manual_edit() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let original = "{not json";
        std::fs::write(cfg_dir.join("opencode.json"), original).unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(hint) => assert!(hint.contains("not valid JSON")),
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap(),
            original
        );
    }

    #[test]
    fn opencode_drops_empty_mcp_object() {
        let _g = EnvGuard::clear(&["XDG_CONFIG_HOME"]);
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".config/opencode");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("opencode.json"),
            r#"{"mcp": {"agent-repertoire": {"type": "local"}}}"#,
        )
        .unwrap();
        let out =
            deconfigure_mcp_for(SkillTarget::Opencode, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(cfg_dir.join("opencode.json")).unwrap())
                .unwrap();
        assert!(v.get("mcp").is_none());
    }

    #[test]
    fn claude_removes_existing_entry_and_keeps_other_mcps() {
        let home = tempfile::tempdir().unwrap();
        let claude_json = home.path().join(".claude.json");
        std::fs::write(
            &claude_json,
            r#"{"mcpServers": {"agent-repertoire": {"command": "/bin/rep"}, "other-server": {"command": "/bin/x"}}, "numStartups": 3}"#,
        )
        .unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Claude, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert!(v["mcpServers"].get("agent-repertoire").is_none());
        assert_eq!(v["mcpServers"]["other-server"]["command"], "/bin/x");
        assert_eq!(v["numStartups"], 3);
    }

    #[test]
    fn claude_no_entry_is_not_present() {
        let home = tempfile::tempdir().unwrap();
        let claude_json = home.path().join(".claude.json");
        let original = r#"{"mcpServers": {"other-server": {"command": "/bin/x"}}}"#;
        std::fs::write(&claude_json, original).unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Claude, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
        assert_eq!(std::fs::read_to_string(&claude_json).unwrap(), original);
    }

    #[test]
    fn claude_missing_file_is_not_present() {
        let home = tempfile::tempdir().unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Claude, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
    }

    #[test]
    fn claude_drops_empty_mcp_servers() {
        let home = tempfile::tempdir().unwrap();
        let claude_json = home.path().join(".claude.json");
        std::fs::write(
            &claude_json,
            r#"{"mcpServers": {"agent-repertoire": {"command": "/bin/rep"}}}"#,
        )
        .unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Claude, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert!(v.get("mcpServers").is_none());
    }

    #[test]
    fn claude_malformed_json_needs_manual_edit() {
        let home = tempfile::tempdir().unwrap();
        let claude_json = home.path().join(".claude.json");
        let original = "{not json";
        std::fs::write(&claude_json, original).unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Claude, home.path()).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(hint) => assert!(hint.contains("not valid JSON")),
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        assert_eq!(std::fs::read_to_string(&claude_json).unwrap(), original);
    }

    #[test]
    fn codex_removes_existing_entry_and_keeps_other_settings() {
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".codex");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("config.toml"),
            "other_key = 1\n[mcp_servers]\n[mcp_servers.agent-repertoire]\ncommand = \"/bin/rep\"\nargs = [\"mcp\"]\n[mcp_servers.other]\ncommand = \"/bin/x\"\n",
        )
        .unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Codex, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let after = std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap();
        assert!(!after.contains("agent-repertoire"));
        assert!(after.contains("[mcp_servers.other]"));
        assert!(after.contains("other_key = 1"));
    }

    #[test]
    fn codex_no_entry_is_not_present() {
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".codex");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let original = "[mcp_servers.other]\ncommand = \"/bin/x\"\n";
        std::fs::write(cfg_dir.join("config.toml"), original).unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Codex, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn codex_missing_file_is_not_present() {
        let home = tempfile::tempdir().unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Codex, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::NotPresent);
    }

    #[test]
    fn codex_deconfigure_malformed_toml_needs_manual_edit() {
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".codex");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        let original = "[[[broken";
        std::fs::write(cfg_dir.join("config.toml"), original).unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Codex, home.path()).unwrap();
        match out.action {
            SetupAction::NeedsManualEdit(hint) => assert!(hint.contains("not valid TOML")),
            other => panic!("expected NeedsManualEdit, got {:?}", other),
        }
        assert_eq!(
            std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn codex_preserves_comments_outside_mcp() {
        let home = tempfile::tempdir().unwrap();
        let cfg_dir = home.path().join(".codex");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(
            cfg_dir.join("config.toml"),
            "# top-level comment\nother_key = 1\n[mcp_servers.agent-repertoire]\ncommand = \"/bin/rep\"\n",
        )
        .unwrap();
        let out = deconfigure_mcp_for(SkillTarget::Codex, home.path()).unwrap();
        assert_eq!(out.action, SetupAction::Removed);
        let after = std::fs::read_to_string(cfg_dir.join("config.toml")).unwrap();
        assert!(after.contains("# top-level comment"));
        assert!(!after.contains("agent-repertoire"));
    }
}
