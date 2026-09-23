use crate::agentsetup::SetupAction;
use crate::registry::Registry;
use crate::{InvalidArgumentIssues, RepError, Result};
use clap::{Parser, Subcommand};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "rep", version, about = "Agent Repertoire — a personal repertoire of operational tools for AI agents")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List all tools in the repertoire
    List,
    /// Search tools by free-text query
    Search {
        query: String,
    },
    /// Show the full definition of a tool
    Inspect {
        name: String,
    },
    /// Run a tool with arguments as key=value pairs
    Run {
        name: String,
        #[arg(value_name = "K=V")]
        args: Vec<String>,
        #[arg(long)]
        timeout_seconds: Option<u64>,
    },
    /// Create a tool from a tool.yaml file (script read from <yaml_dir>/<entrypoint>)
    Create {
        #[arg(long)]
        file: PathBuf,
    },
    /// Print the source script of a tool (--yaml prints metadata generated from the catalog)
    Source {
        name: String,
        #[arg(long)]
        yaml: bool,
    },
    /// Update a tool's script and/or metadata in place
    Update {
        name: String,
        #[arg(long)]
        script: Option<PathBuf>,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Remove a tool from the repertoire. Asks for confirmation
    /// interactively; --yes skips the prompt (required when stdin
    /// is not a terminal).
    Remove {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    /// Run the MCP server over stdio
    Mcp,
    /// Install the Agent Repertoire Skill and register the MCP server for an
    /// agent (or all three).
    Install {
        #[arg(long, value_enum, conflicts_with = "all")]
        target: Option<crate::skill::SkillTarget>,
        #[arg(long)]
        all: bool,
    },
    /// Uninstall agent-repertoire: removes the Skill directory and the
    /// `agent-repertoire` MCP entry from every configured agent. With
    /// --all, also deletes the data directory after confirmation.
    Uninstall {
        /// Also delete the data directory
        /// ($AGENT_REPERTOIRE_HOME or ~/.agent-repertoire/), which contains
        /// every stored tool. This is destructive and irreversible;
        /// requires confirmation unless --yes is also passed.
        #[arg(long)]
        all: bool,
        /// Skip the confirmation prompt when --all is used. For
        /// non-interactive scripts and CI. Requires --all.
        #[arg(long, requires = "all")]
        yes: bool,
    },
}

pub fn parse_kv(s: &str) -> Option<(String, serde_json::Value)> {
    let (key, raw) = s.split_once('=')?;
    let value = serde_json::from_str(raw).unwrap_or(serde_json::Value::String(raw.to_string()));
    Some((key.to_string(), value))
}

pub fn build_args(pairs: &[String]) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    for pair in pairs {
        let (key, value) = parse_kv(pair).ok_or_else(|| {
            RepError::InvalidArguments(InvalidArgumentIssues(vec![format!(
                "expected key=value, got '{}'",
                pair
            )]))
        })?;
        map.insert(key, value);
    }
    Ok(serde_json::Value::Object(map))
}

fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
    {
        if let Ok(rest) = path.strip_prefix(&home) {
            if rest.as_os_str().is_empty() {
                return "~".to_string();
            }
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

fn wrap_indent(text: &str, width: usize, indent: &str) -> Vec<String> {
    let indent_len = indent.chars().count();
    let avail = width.saturating_sub(indent_len).max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::from(indent);
    let mut current_len = indent_len;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if current_len == indent_len {
            current.push_str(word);
            current_len += word_len;
        } else if current_len + 1 + word_len <= avail {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
        } else {
            lines.push(std::mem::take(&mut current));
            current = format!("{}{}", indent, word);
            current_len = indent_len + word_len;
        }
    }
    if current_len > indent_len {
        lines.push(current);
    }
    lines
}

fn metadata_line(t: &crate::models::ToolSummary) -> String {
    let mut segments = vec![t.language.to_string()];
    if t.usage_count == 0 {
        segments.push("never used".to_string());
    } else {
        segments.push(format!("used {}x", t.usage_count));
        if let Some(ts) = &t.last_used_at {
            segments.push(format!("last used {}", ts.get(..10).unwrap_or(ts)));
        }
    }
    if !t.keywords.is_empty() {
        segments.push(format!("keywords: {}", t.keywords.join(", ")));
    }
    format!("    {}", segments.join(" · "))
}

fn tool_count_footer(n: usize) -> String {
    if n == 1 {
        "1 tool".to_string()
    } else {
        format!("{} tools", n)
    }
}

fn available_width(term_width: Option<u16>) -> usize {
    let w = term_width.unwrap_or(100) as usize;
    w.saturating_sub(4).max(40)
}

fn stdout_wrap_width() -> usize {
    available_width(terminal_size::terminal_size().map(|(w, _)| w.0))
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::List => {
            let registry = Registry::open(None)?;
            let tools = registry.list()?;
            if tools.is_empty() {
                println!("no tools in the repertoire");
            } else {
                let width = stdout_wrap_width();
                for (i, t) in tools.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("{}", t.name);
                    println!("{}", metadata_line(t));
                    for line in wrap_indent(&t.description, width, "    ") {
                        println!("{}", line);
                    }
                }
                println!("{}", tool_count_footer(tools.len()));
            }
        }
        Command::Search { query } => {
            let registry = Registry::open(None)?;
            let hits = registry.search(&query, crate::search::DEFAULT_LIMIT)?;
            if hits.is_empty() {
                println!("no tools match '{}'", query);
            } else {
                for h in &hits {
                    println!("{:<40} {:>.3}  {}", h.name, h.score, h.description);
                }
            }
        }
        Command::Inspect { name } => {
            let registry = Registry::open(None)?;
            let tool = registry.inspect(&name)?;
            println!("name:        {}", tool.name);
            println!("description: {}", tool.description);
            println!("language:    {}", tool.language);
            println!("entrypoint:  {}", tool.entrypoint);
            if !tool.keywords.is_empty() {
                println!("keywords:    {}", tool.keywords.join(", "));
            }
            if !tool.commands.is_empty() {
                println!("commands:    {}", tool.commands.join(", "));
            }
            if !tool.examples.is_empty() {
                println!("examples:");
                for ex in &tool.examples {
                    println!("    {}", serde_json::to_string(ex).unwrap_or_default());
                }
            }
            println!(
                "capabilities: read_only={} destructive={} network={} filesystem={} requires_credentials={}",
                tool.capabilities.read_only,
                tool.capabilities.destructive,
                tool.capabilities.network,
                tool.capabilities.filesystem,
                tool.capabilities.requires_credentials
            );
            println!("usage:       {}x", tool.usage_count);
            match tool.timeout_seconds {
                Some(t) => println!("timeout:     {}s", t),
                None => println!("timeout:     default"),
            }
        }
        Command::Run {
            name,
            args,
            timeout_seconds,
        } => {
            let registry = Registry::open(None)?;
            let arguments = build_args(&args)?;
            let out = registry.run(&name, &arguments, timeout_seconds)?;
            if out.timed_out {
                eprintln!("tool timed out after {}ms", out.duration_ms);
            } else if out.exit_code != Some(0) {
                eprintln!("tool exited with {:?}", out.exit_code);
            }
            if !out.stderr.is_empty() {
                eprint!("{}", out.stderr);
            }
            if !out.stdout.is_empty() {
                print!("{}", out.stdout);
            }
            if out.timed_out || out.exit_code != Some(0) {
                return Err(RepError::Spawn("tool execution failed".into()));
            }
        }
        Command::Create { file } => {
            let registry = Registry::open(None)?;
            let raw = std::fs::read_to_string(&file)?;
            let yaml: crate::models::ToolYaml = serde_yml::from_str(&raw)?;
            let script_path = file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(&yaml.entrypoint);
            let script_content = std::fs::read_to_string(&script_path).map_err(|e| {
                RepError::Spawn(format!(
                    "cannot read script {}: {}",
                    script_path.display(),
                    e
                ))
            })?;
            let spec = crate::models::NewTool {
                name: yaml.name,
                description: yaml.description,
                language: yaml.language,
                script: script_content,
                entrypoint: Some(yaml.entrypoint),
                input_schema: yaml.input_schema,
                keywords: yaml.keywords,
                commands: yaml.commands,
                examples: yaml.examples,
                capabilities: yaml.capabilities,
                timeout_seconds: yaml.timeout_seconds,
            };
            let tool = registry.create_tool(spec)?;
            println!("created {} (version {})", tool.name, tool.version);
        }
        Command::Source { name, yaml } => {
            let registry = Registry::open(None)?;
            let src = registry.get_tool_source(&name)?;
            if yaml {
                print!("{}", src.metadata_yaml);
            } else {
                print!("{}", src.script);
            }
        }
        Command::Update { name, script, file } => {
            if script.is_none() && file.is_none() {
                return Err(RepError::InvalidArguments(InvalidArgumentIssues(vec![
                    "pass --script and/or --file".to_string(),
                ])));
            }
            let registry = Registry::open(None)?;
            let mut update = crate::registry::ToolUpdate::default();
            if let Some(p) = script {
                update.script = Some(std::fs::read_to_string(&p)?);
            }
            if let Some(f) = file {
                let raw = std::fs::read_to_string(&f)?;
                let y: crate::models::ToolYaml = serde_yml::from_str(&raw)?;
                update.description = Some(y.description);
                update.keywords = Some(y.keywords);
                update.commands = Some(y.commands);
                update.input_schema = Some(y.input_schema);
                update.examples = Some(y.examples);
                update.capabilities = Some(y.capabilities);
                if y.timeout_seconds.is_some() {
                    update.timeout_seconds = y.timeout_seconds;
                }
            }
            let tool = registry.update_tool(&name, update)?;
            println!("updated {} (version {})", tool.name, tool.version);
        }
        Command::Remove { name, yes } => {
            let proceed = if yes {
                true
            } else if std::io::stdin().is_terminal() {
                eprint!(
                    "This will permanently remove tool '{}' and its script directory.\nType 'yes' to confirm: ",
                    name
                );
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).map_err(|e| {
                    RepError::Spawn(format!("cannot read confirmation: {}", e))
                })?;
                input.trim().eq_ignore_ascii_case("yes")
            } else {
                return Err(RepError::Spawn(
                    "refusing to remove non-interactively; pass --yes to confirm".into(),
                ));
            };
            if proceed {
                let registry = Registry::open(None)?;
                registry.remove(&name)?;
                println!("removed {}", name);
            } else {
                println!("aborted: nothing removed");
            }
        }
        Command::Mcp => {
            crate::mcp::serve()?;
        }
        Command::Install { target, all } => {
            let selected = match (all, target) {
                (true, _) => crate::skill::SkillTarget::all().to_vec(),
                (false, Some(t)) => vec![t],
                (false, None) => match crate::prompt::select_providers()? {
                    crate::prompt::PromptOutcome::Selected(list) => list,
                    crate::prompt::PromptOutcome::Aborted => {
                        println!("aborted: nothing installed");
                        return Ok(());
                    }
                },
            };
            let exe = std::env::current_exe()?;
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or_else(|| RepError::Spawn("cannot determine home directory; set HOME".into()))?;
            for t in selected {
                crate::skill::install_skill(t)?;
                let dir = crate::skill::target_dir(t)?;
                println!("installed {}", t.as_str());
                println!("  skill at {}", display_path(&dir));
                match crate::agentsetup::configure_mcp_for(t, &home, &exe) {
                    Ok(outcome) => match &outcome.action {
                        SetupAction::Created => {
                            println!("  mcp server in {}", display_path(&outcome.config_path));
                        }
                        SetupAction::Patched => {
                            println!(
                                "  mcp config updated in {}",
                                display_path(&outcome.config_path)
                            );
                        }
                        SetupAction::AlreadyConfigured => {
                            println!(
                                "  mcp server already configured in {}",
                                display_path(&outcome.config_path)
                            );
                        }
                        SetupAction::NeedsManualEdit(hint) => {
                            eprintln!(
                                "warning: could not edit {} safely; add manually:",
                                display_path(&outcome.config_path)
                            );
                            eprintln!("{}", hint);
                        }
                        SetupAction::Removed | SetupAction::NotPresent => {}
                    },
                    Err(e) => {
                        eprintln!(
                            "error: failed to configure mcp for {}: {}",
                            t.as_str(),
                            e
                        );
                    }
                }
            }
        }
        Command::Uninstall { all, yes } => {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .ok_or_else(|| RepError::Spawn("cannot determine home directory; set HOME".into()))?;
            let mut removed_any = false;
            for t in crate::skill::SkillTarget::all().to_vec() {
                let mut details: Vec<String> = Vec::new();
                let dir = crate::skill::target_dir(t)?;
                if dir.exists() {
                    crate::skill::uninstall_skill(t)?;
                    details.push(format!("  skill at {}", display_path(&dir)));
                }
                match crate::agentsetup::deconfigure_mcp_for(t, &home) {
                    Ok(outcome) => match &outcome.action {
                        SetupAction::Removed => {
                            details.push(format!(
                                "  mcp server in {}",
                                display_path(&outcome.config_path)
                            ));
                        }
                        SetupAction::NeedsManualEdit(hint) => {
                            eprintln!(
                                "warning: could not edit {} safely; remove the agent-repertoire block by hand\n{}",
                                display_path(&outcome.config_path),
                                hint
                            );
                        }
                        _ => {}
                    },
                    Err(e) => {
                        eprintln!("error: failed to remove mcp for {}: {}", t.as_str(), e);
                    }
                }
                if !details.is_empty() {
                    removed_any = true;
                    println!("uninstalled {}", t.as_str());
                    for line in &details {
                        println!("{}", line);
                    }
                }
            }
            let mut data_dir_present = false;
            if all {
                let data_root = data_root_path()?;
                if data_root.exists() {
                    data_dir_present = true;
                    let proceed = if yes {
                        true
                    } else if std::io::stdin().is_terminal() {
                        eprint!(
                            "This will permanently delete {} and all stored tools.\nType 'yes' to confirm: ",
                            display_path(&data_root)
                        );
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input).map_err(|e| {
                            RepError::Spawn(format!("cannot read confirmation: {}", e))
                        })?;
                        input.trim().eq_ignore_ascii_case("yes")
                    } else {
                        return Err(RepError::Spawn(
                            "refusing to delete the data directory non-interactively; pass --yes to confirm"
                                .into(),
                        ));
                    };
                    if proceed {
                        std::fs::remove_dir_all(&data_root)?;
                        println!("removed {}", display_path(&data_root));
                    } else {
                        println!("aborted: nothing deleted");
                    }
                }
            }
            if !removed_any && !data_dir_present {
                println!("nothing to uninstall");
            }
        }
    }
    Ok(())
}

fn data_root_path() -> Result<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("AGENT_REPERTOIRE_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
    {
        return Ok(p);
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| RepError::Spawn("cannot determine home directory; set HOME".into()))?;
    Ok(home.join(".agent-repertoire"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kv_parsing() {
        assert_eq!(
            parse_kv("service=payments"),
            Some(("service".into(), json!("payments")))
        );
        assert_eq!(parse_kv("count=3"), Some(("count".into(), json!(3))));
        assert_eq!(
            parse_kv("flag=true"),
            Some(("flag".into(), json!(true)))
        );
        assert_eq!(
            parse_kv("config={\"a\": 1}"),
            Some(("config".into(), json!({"a": 1})))
        );
        assert_eq!(parse_kv("novalue"), None);
    }

    #[test]
    fn args_building() {
        let args = build_args(&["service=payments".into(), "duration=30m".into()]).unwrap();
        assert_eq!(
            args,
            json!({"service": "payments", "duration": "30m"})
        );
        let args = build_args(&["service=override".into()]).unwrap();
        assert_eq!(args, json!({"service": "override"}));
        assert!(build_args(&["bad".into()]).is_err());
    }

    #[test]
    fn display_path_abbreviates_under_home() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("HOME", "/home/fakeuser");
        assert_eq!(
            display_path(Path::new("/home/fakeuser/.claude/skills/agent-repertoire")),
            "~/.claude/skills/agent-repertoire"
        );
        assert_eq!(display_path(Path::new("/home/fakeuser")), "~");
        assert_eq!(
            display_path(Path::new("/home/fakeuserish/x")),
            "/home/fakeuserish/x"
        );
        std::env::remove_var("HOME");
    }

    #[test]
    fn display_path_without_home_returns_full_path() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::remove_var("HOME");
        assert_eq!(
            display_path(Path::new("/etc/agent.json")),
            "/etc/agent.json"
        );
    }

    #[test]
    fn wrap_indent_short_text_single_line() {
        assert_eq!(
            super::wrap_indent("hello world", 100, "    "),
            vec!["    hello world"]
        );
    }

    #[test]
    fn wrap_indent_wraps_and_keeps_indent() {
        let lines = super::wrap_indent("aaaa bbbb cccc dddd", 14, "  ");
        assert!(lines.iter().all(|l| l.starts_with("  ")));
        assert_eq!(lines[0], "  aaaa bbbb");
        assert_eq!(lines[1], "  cccc dddd");
    }

    #[test]
    fn wrap_indent_never_splits_long_words() {
        assert_eq!(
            super::wrap_indent("abcdefghij x", 10, ""),
            vec!["abcdefghij", "x"]
        );
    }

    #[test]
    fn wrap_indent_collapses_whitespace() {
        assert_eq!(
            super::wrap_indent("a   b\n\tc", 100, "    "),
            vec!["    a b c"]
        );
    }

    fn list_summary(
        keywords: Vec<&str>,
        usage: i64,
        last: Option<&str>,
    ) -> crate::models::ToolSummary {
        crate::models::ToolSummary {
            name: "t".into(),
            description: "d".into(),
            language: crate::models::Language::Bash,
            usage_count: usage,
            last_used_at: last.map(str::to_string),
            keywords: keywords.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn metadata_line_full() {
        assert_eq!(
            super::metadata_line(&list_summary(
                vec!["a", "b"],
                3,
                Some("2026-09-20T10:00:00Z")
            )),
            "    bash · used 3x · last used 2026-09-20 · keywords: a, b"
        );
    }

    #[test]
    fn metadata_line_never_used_omits_last_used() {
        assert_eq!(
            super::metadata_line(&list_summary(vec![], 0, None)),
            "    bash · never used"
        );
        assert_eq!(
            super::metadata_line(&list_summary(vec!["k"], 0, None)),
            "    bash · never used · keywords: k"
        );
    }

    #[test]
    fn footer_singular_and_plural() {
        assert_eq!(super::tool_count_footer(1), "1 tool");
        assert_eq!(super::tool_count_footer(2), "2 tools");
    }

    #[test]
    fn available_width_uses_fallback_and_floors_at_40() {
        assert_eq!(super::available_width(None), 96);
        assert_eq!(super::available_width(Some(200)), 196);
        assert_eq!(super::available_width(Some(10)), 40);
    }
}
