use crate::models::{NewTool, Tool, ToolRecord, ToolYaml};
use crate::storage::Storage;
use crate::validation;
use crate::{RepError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ToolUpdate {
    pub script: Option<String>,
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub commands: Option<Vec<String>>,
    pub input_schema: Option<serde_json::Value>,
    pub examples: Option<Vec<serde_json::Value>>,
    pub capabilities: Option<crate::models::Capabilities>,
    pub timeout_seconds: Option<u64>,
}

pub struct Registry {
    root: PathBuf,
    tools_dir: PathBuf,
    storage: Storage,
}

impl Registry {
    pub fn open(root: Option<PathBuf>) -> Result<Self> {
        let root = root
            .or_else(|| {
                std::env::var_os("AGENT_REPERTOIRE_HOME")
                    .and_then(|s| if s.is_empty() { None } else { Some(PathBuf::from(s)) })
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".agent-repertoire"))
            })
            .ok_or_else(|| {
                RepError::Spawn(
                    "cannot determine data directory; set AGENT_REPERTOIRE_HOME".into(),
                )
            })?;
        let tools_dir = root.join("tools");
        std::fs::create_dir_all(&tools_dir)?;
        let storage = Storage::open(&root.join("repertoire.db"))?;
        Ok(Self {
            root,
            tools_dir,
            storage,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tools_dir(&self) -> &Path {
        &self.tools_dir
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    fn tool_dir(&self, name: &str) -> PathBuf {
        self.tools_dir.join(name)
    }

    pub fn create_tool(&self, mut spec: NewTool) -> Result<Tool> {
        spec.name = spec.name.replace(' ', "_");
        validation::validate_new_tool(&spec)?;
        if self.storage.get_tool(&spec.name)?.is_some() {
            return Err(RepError::AlreadyExists(spec.name));
        }
        let dir = self.tool_dir(&spec.name);
        if dir.exists() {
            return Err(RepError::AlreadyExists(format!(
                "{} (directory exists on disk)",
                spec.name
            )));
        }
        let entrypoint = spec
            .entrypoint
            .clone()
            .unwrap_or_else(|| spec.language.default_entrypoint().to_string());
        let now = chrono::Utc::now().to_rfc3339();
        let record = ToolRecord {
            id: 0,
            name: spec.name.clone(),
            description: spec.description.clone(),
            language: spec.language,
            entrypoint: entrypoint.clone(),
            input_schema: if spec.input_schema.is_null() {
                serde_json::json!({})
            } else {
                spec.input_schema.clone()
            },
            capabilities: spec.capabilities.clone(),
            keywords: spec.keywords.clone(),
            commands: spec.commands.clone(),
            examples: spec.examples.clone(),
            timeout_seconds: spec.timeout_seconds,
            version: 1,
            created_at: now.clone(),
            updated_at: now,
            usage_count: 0,
            last_used_at: None,
        };

        let write_result = (|| -> Result<()> {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join(&entrypoint), &spec.script)?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }

        match self.storage.insert_tool(&record) {
            Ok(inserted) => Ok(inserted.into()),
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    }

    pub fn list(&self) -> Result<Vec<crate::models::ToolSummary>> {
        Ok(self
            .storage
            .list_tools()?
            .into_iter()
            .map(|rec| crate::models::ToolSummary {
                name: rec.name,
                description: rec.description,
                language: rec.language,
                usage_count: rec.usage_count,
                last_used_at: rec.last_used_at,
                keywords: rec.keywords,
            })
            .collect())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<crate::models::SearchHit>> {
        crate::search::run_search(&self.storage, query, limit)
    }

    pub fn inspect(&self, name: &str) -> Result<Tool> {
        let rec = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| RepError::NotFound(name.to_string()))?;
        Ok(rec.into())
    }

    pub fn get_tool_source(&self, name: &str) -> Result<crate::models::ToolSource> {
        let rec = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| RepError::NotFound(name.to_string()))?;
        let dir = self.tool_dir(name);
        let script = std::fs::read_to_string(dir.join(&rec.entrypoint)).map_err(|e| {
            RepError::Drift(format!(
                "cannot read entrypoint '{}' of '{}': {}",
                rec.entrypoint, name, e
            ))
        })?;
        let metadata_yaml = serde_yml::to_string(&ToolYaml::from_record(&rec))?;
        Ok(crate::models::ToolSource {
            name: rec.name,
            language: rec.language,
            entrypoint: rec.entrypoint,
            script,
            metadata_yaml,
            version: rec.version,
        })
    }

    fn restore_files(restored: &[(PathBuf, Option<String>)]) {
        for (path, old) in restored.iter().rev() {
            match old {
                Some(content) => {
                    let _ = std::fs::write(path, content);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    pub fn update_tool(&self, name: &str, update: ToolUpdate) -> Result<Tool> {
        let rec = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| RepError::NotFound(name.to_string()))?;
        let dir = self.tool_dir(name);
        let script_path = dir.join(&rec.entrypoint);
        let old_script = std::fs::read_to_string(&script_path).map_err(|e| {
            RepError::Drift(format!("cannot read entrypoint '{}': {}", rec.entrypoint, e))
        })?;

        let script = update.script.clone().unwrap_or_else(|| old_script.clone());
        let description = update.description.clone().unwrap_or_else(|| rec.description.clone());
        let keywords = update.keywords.clone().unwrap_or_else(|| rec.keywords.clone());
        let commands = update.commands.clone().unwrap_or_else(|| rec.commands.clone());
        let input_schema = update.input_schema.clone().unwrap_or_else(|| rec.input_schema.clone());
        let examples = update.examples.clone().unwrap_or_else(|| rec.examples.clone());
        let capabilities = update.capabilities.clone().unwrap_or_else(|| rec.capabilities.clone());
        let timeout_seconds = update.timeout_seconds.or(rec.timeout_seconds);

        validation::validate_new_tool(&NewTool {
            name: rec.name.clone(),
            description: description.clone(),
            language: rec.language,
            script: script.clone(),
            entrypoint: Some(rec.entrypoint.clone()),
            input_schema: input_schema.clone(),
            keywords: keywords.clone(),
            commands: commands.clone(),
            examples: examples.clone(),
            capabilities: capabilities.clone(),
            timeout_seconds,
        })?;

        let mut restored: Vec<(PathBuf, Option<String>)> = Vec::new();
        let write_result = (|| -> Result<()> {
            if update.script.is_some() {
                atomic_write(&script_path, &script, &mut restored)?;
            }
            Ok(())
        })();
        if let Err(e) = write_result {
            Self::restore_files(&restored);
            return Err(e);
        }

        let mut new_rec = rec.clone();
        new_rec.description = description;
        new_rec.input_schema = input_schema;
        new_rec.capabilities = capabilities;
        new_rec.keywords = keywords;
        new_rec.commands = commands;
        new_rec.examples = examples;
        new_rec.timeout_seconds = timeout_seconds;
        new_rec.version = rec.version + 1;
        new_rec.updated_at = chrono::Utc::now().to_rfc3339();
        match self.storage.update_tool(&new_rec) {
            Ok(stored) => Ok(stored.into()),
            Err(e) => {
                Self::restore_files(&restored);
                Err(e)
            }
        }
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        if !self.storage.delete_tool(name)? {
            return Err(RepError::NotFound(name.to_string()));
        }
        let dir = self.tool_dir(name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    pub fn run(
        &self,
        name: &str,
        args: &serde_json::Value,
        timeout: Option<u64>,
    ) -> Result<crate::models::RunOutcome> {
        let rec = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| RepError::NotFound(name.to_string()))?;
        validation::validate_arguments(&rec.input_schema, args)?;
        let effective = timeout
            .or(rec.timeout_seconds)
            .unwrap_or(crate::runner::DEFAULT_TIMEOUT.as_secs());
        let req = crate::runner::ExecutionRequest {
            interpreter: rec.language.interpreter().to_string(),
            script: self.tool_dir(name).join(&rec.entrypoint),
            stdin_json: args.clone(),
            timeout: std::time::Duration::from_secs(effective),
        };
        let outcome = crate::runner::execute(req);
        let _ = self.storage.record_usage(name);
        outcome
    }

}

fn atomic_write(
    path: &Path,
    content: &str,
    restored: &mut Vec<(PathBuf, Option<String>)>,
) -> Result<()> {
    let old = std::fs::read_to_string(path).ok();
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => {
            restored.push((path.to_path_buf(), old));
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, Language, NewTool};
    use serde_json::json;

    pub(crate) fn temp_registry() -> (tempfile::TempDir, Registry) {
        let dir = tempfile::tempdir().unwrap();
        let reg = Registry::open(Some(dir.path().to_path_buf())).unwrap();
        (dir, reg)
    }

    pub(crate) fn bash_tool(name: &str) -> NewTool {
        NewTool {
            name: name.into(),
            description: format!("Tool {}", name),
            language: Language::Bash,
            script: "#!/usr/bin/env bash\necho '{\"n\": 1}'\n".into(),
            entrypoint: None,
            input_schema: json!({"type": "object", "properties": {"x": {"type": "string"}}}),
            keywords: vec!["test".into()],
            commands: vec!["echo".into()],
            examples: vec![json!({"x": "a"})],
            capabilities: Capabilities::default(),
            timeout_seconds: Some(5),
        }
    }

    #[test]
    fn open_creates_layout() {
        let (dir, reg) = temp_registry();
        assert!(dir.path().join("repertoire.db").exists());
        assert!(dir.path().join("tools").is_dir());
        assert_eq!(reg.tools_dir(), dir.path().join("tools"));
    }

    #[test]
    fn create_writes_script_and_db_only() {
        let (dir, reg) = temp_registry();
        let tool = reg.create_tool(bash_tool("alpha_create")).unwrap();
        assert!(tool.id > 0);
        assert_eq!(tool.version, 1);
        assert_eq!(tool.entrypoint, "run.sh");
        let tdir = dir.path().join("tools").join("alpha_create");
        assert!(tdir.join("run.sh").exists());
        assert!(!tdir.join("tool.yaml").exists());
        let stored = reg.storage().get_tool("alpha_create").unwrap().unwrap();
        assert_eq!(stored.keywords, vec!["test".to_string()]);
        assert_eq!(stored.commands, vec!["echo".to_string()]);
        assert_eq!(stored.examples.len(), 1);
        assert_eq!(stored.timeout_seconds, Some(5));
    }

    #[test]
    fn duplicate_name_rejected() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("dup_tool")).unwrap();
        let err = reg.create_tool(bash_tool("dup_tool")).unwrap_err();
        assert!(matches!(err, crate::RepError::AlreadyExists(_)));
    }

    #[test]
    fn existing_directory_rejected() {
        let (dir, reg) = temp_registry();
        let tdir = dir.path().join("tools").join("orphan_tool");
        std::fs::create_dir_all(&tdir).unwrap();
        let err = reg.create_tool(bash_tool("orphan_tool")).unwrap_err();
        assert!(matches!(err, crate::RepError::AlreadyExists(_)));
    }

    #[test]
    fn invalid_spec_rejected_before_writing() {
        let (dir, reg) = temp_registry();
        let mut spec = bash_tool("bad_spec");
        spec.name = "Bad".into();
        assert!(reg.create_tool(spec).is_err());
        assert!(!dir.path().join("tools").join("Bad").exists());
    }

    #[test]
    fn name_with_spaces_is_normalized_to_underscores() {
        let (dir, reg) = temp_registry();
        let tool = reg.create_tool(bash_tool("hello world tool")).unwrap();
        assert_eq!(tool.name, "hello_world_tool");
        assert!(dir.path().join("tools").join("hello_world_tool").is_dir());
        assert!(!dir.path().join("tools").join("hello world tool").exists());
        let stored = reg.storage().get_tool("hello_world_tool").unwrap().unwrap();
        assert_eq!(stored.name, "hello_world_tool");
    }

    #[test]
    fn env_var_root_override() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("AGENT_REPERTOIRE_HOME", dir.path().to_str().unwrap());
        let reg = Registry::open(None).unwrap();
        assert_eq!(reg.root(), dir.path());
        drop(reg);
        drop(_guard);
    }

    struct EnvGuard {
        key: &'static str,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let lock = crate::test_support::ENV_LOCK.lock().unwrap();
            std::env::set_var(key, value);
            Self {
                key,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            std::env::remove_var(self.key);
        }
    }

    #[test]
    fn list_search_inspect_remove() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("beta_list")).unwrap();
        reg.create_tool(bash_tool("gamma_other")).unwrap();

        let list = reg.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|t| t.usage_count == 0));
        assert!(list
            .iter()
            .all(|t| t.keywords == vec!["test".to_string()]));

        let hits = reg.search("test", 10).unwrap();
        assert_eq!(hits.len(), 2);
        let hits = reg.search("beta", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "beta_list");

        let tool = reg.inspect("beta_list").unwrap();
        assert_eq!(tool.keywords, vec!["test".to_string()]);
        assert_eq!(tool.commands, vec!["echo".to_string()]);
        assert_eq!(tool.examples.len(), 1);
        assert_eq!(tool.language, Language::Bash);

        assert!(matches!(
            reg.inspect("missing_tool").unwrap_err(),
            crate::RepError::NotFound(_)
        ));

        reg.remove("beta_list").unwrap();
        assert!(matches!(
            reg.inspect("beta_list").unwrap_err(),
            crate::RepError::NotFound(_)
        ));
        assert_eq!(reg.list().unwrap().len(), 1);
        assert!(reg.search("beta", 10).unwrap().is_empty());
        assert!(matches!(
            reg.remove("beta_list").unwrap_err(),
            crate::RepError::NotFound(_)
        ));
    }

    #[test]
    fn run_executes_bash_tool_and_records_usage() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("run_bash")).unwrap();
        let out = reg.run("run_bash", &json!({"x": "a"}), None).unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out.stdout).unwrap()["n"], 1);
        let tool = reg.inspect("run_bash").unwrap();
        assert_eq!(tool.usage_count, 1);
        assert!(tool.last_used_at.is_some());
    }

    #[test]
    fn run_executes_python_tool_with_stdin_json() {
        let (_dir, reg) = temp_registry();
        let mut spec = bash_tool("run_python");
        spec.language = Language::Python;
        spec.script = "import json, sys\nargs = json.load(sys.stdin)\nprint(json.dumps({\"got\": args.get(\"x\", \"\")}))\n".into();
        reg.create_tool(spec).unwrap();
        let out = reg.run("run_python", &json!({"x": "hello"}), None).unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&out.stdout).unwrap()["got"],
            "hello"
        );
    }

    #[test]
    fn run_rejects_invalid_arguments() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("run_invalid")).unwrap();
        let err = reg.run("run_invalid", &json!({"x": 5}), None).unwrap_err();
        assert!(matches!(err, crate::RepError::InvalidArguments(_)));
        let tool = reg.inspect("run_invalid").unwrap();
        assert_eq!(tool.usage_count, 0);
    }

    #[test]
    fn run_unknown_tool_is_not_found() {
        let (_dir, reg) = temp_registry();
        assert!(matches!(
            reg.run("ghost_tool", &json!({}), None).unwrap_err(),
            crate::RepError::NotFound(_)
        ));
    }

    #[test]
    fn run_timeout_from_stored_record() {
        let (_dir, reg) = temp_registry();
        let mut spec = bash_tool("run_slow");
        spec.script = "sleep 2".into();
        spec.timeout_seconds = Some(1);
        reg.create_tool(spec).unwrap();
        let out = reg.run("run_slow", &json!({}), None).unwrap();
        assert!(out.timed_out);
        let out = reg.run("run_slow", &json!({}), Some(60)).unwrap();
        assert!(!out.timed_out);
        assert_eq!(out.exit_code, Some(0));
    }

    #[test]
    fn run_nonzero_exit_still_returns_outcome() {
        let (_dir, reg) = temp_registry();
        let mut spec = bash_tool("run_fail");
        spec.script = "echo boom >&2\nexit 3\n".into();
        reg.create_tool(spec).unwrap();
        let out = reg.run("run_fail", &json!({}), None).unwrap();
        assert_eq!(out.exit_code, Some(3));
        assert_eq!(out.stderr.trim(), "boom");
        assert_eq!(reg.inspect("run_fail").unwrap().usage_count, 1);
    }

    #[test]
    fn get_tool_source_returns_script_and_generated_yaml() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("src_tool")).unwrap();
        let src = reg.get_tool_source("src_tool").unwrap();
        assert_eq!(src.name, "src_tool");
        assert_eq!(src.language, Language::Bash);
        assert_eq!(src.entrypoint, "run.sh");
        assert!(src.script.contains("echo"));
        assert!(src.metadata_yaml.contains("name: src_tool"));
        let parsed: crate::models::ToolYaml =
            serde_yml::from_str(&src.metadata_yaml).unwrap();
        assert_eq!(parsed.keywords, vec!["test".to_string()]);
        assert_eq!(parsed.timeout_seconds, Some(5));
        assert_eq!(src.version, 1);
        assert!(matches!(
            reg.get_tool_source("ghost_src").unwrap_err(),
            crate::RepError::NotFound(_)
        ));
    }

    use crate::registry::ToolUpdate;

    #[test]
    fn update_script_bumps_version_and_keeps_metadata() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_script")).unwrap();
        let tool = reg
            .update_tool(
                "upd_script",
                ToolUpdate {
                    script: Some("#!/usr/bin/env bash\necho '{\"n\": 2}'\n".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(tool.version, 2);
        assert_eq!(tool.description, "Tool upd_script");
        assert_eq!(tool.keywords, vec!["test".to_string()]);
        assert_eq!(tool.timeout_seconds, Some(5));
        let out = reg.run("upd_script", &json!({"x": "a"}), None).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&out.stdout).unwrap()["n"], 2);
    }

    #[test]
    fn update_metadata_only_keeps_script() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_meta")).unwrap();
        let tool = reg
            .update_tool(
                "upd_meta",
                ToolUpdate {
                    description: Some("Better description".into()),
                    keywords: Some(vec!["renamed".into()]),
                    timeout_seconds: Some(42),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(tool.version, 2);
        assert_eq!(tool.description, "Better description");
        assert_eq!(tool.timeout_seconds, Some(42));
        let src = reg.get_tool_source("upd_meta").unwrap();
        assert!(src.script.contains("echo"));
        assert!(src.metadata_yaml.contains("Better description"));
        let hits = reg.search("renamed", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn update_preserves_usage_stats() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_stats")).unwrap();
        reg.run("upd_stats", &json!({"x": "a"}), None).unwrap();
        reg.update_tool(
            "upd_stats",
            ToolUpdate { description: Some("used".into()), ..Default::default() },
        )
        .unwrap();
        let tool = reg.inspect("upd_stats").unwrap();
        assert_eq!(tool.usage_count, 1);
        assert!(tool.last_used_at.is_some());
    }

    #[test]
    fn update_rejects_invalid_merge_without_mutating() {
        let (dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_invalid")).unwrap();
        let script_path = dir.path().join("tools").join("upd_invalid").join("run.sh");
        let before = std::fs::read_to_string(&script_path).unwrap();
        let err = reg
            .update_tool(
                "upd_invalid",
                ToolUpdate { description: Some("   ".into()), ..Default::default() },
            )
            .unwrap_err();
        assert!(matches!(err, crate::RepError::InvalidTool(_)));
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), before);
        assert_eq!(reg.inspect("upd_invalid").unwrap().version, 1);
    }

    #[test]
    fn update_metadata_only_does_not_touch_disk() {
        let (dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_disk")).unwrap();
        let script_path = dir.path().join("tools").join("upd_disk").join("run.sh");
        let before = std::fs::read_to_string(&script_path).unwrap();
        let mtime_before = std::fs::metadata(&script_path).unwrap().modified().unwrap();
        reg.update_tool(
            "upd_disk",
            ToolUpdate { description: Some("only metadata".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&script_path).unwrap(), before);
        assert_eq!(std::fs::metadata(&script_path).unwrap().modified().unwrap(), mtime_before);
        assert_eq!(reg.inspect("upd_disk").unwrap().description, "only metadata");
    }

    #[test]
    fn update_unknown_tool_is_not_found() {
        let (_dir, reg) = temp_registry();
        assert!(matches!(
            reg.update_tool("ghost_upd", ToolUpdate::default()).unwrap_err(),
            crate::RepError::NotFound(_)
        ));
    }

    #[test]
    fn update_twice_bumps_version_to_three() {
        let (_dir, reg) = temp_registry();
        reg.create_tool(bash_tool("upd_twice")).unwrap();
        reg.update_tool("upd_twice", ToolUpdate { description: Some("a".into()), ..Default::default() }).unwrap();
        let tool = reg.update_tool("upd_twice", ToolUpdate { description: Some("b".into()), ..Default::default() }).unwrap();
        assert_eq!(tool.version, 3);
    }
}
