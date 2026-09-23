use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    Bash,
}

impl Language {
    pub fn interpreter(self) -> &'static str {
        match self {
            Language::Python => "python3",
            Language::Bash => "bash",
        }
    }

    pub fn default_entrypoint(self) -> &'static str {
        match self {
            Language::Python => "run.py",
            Language::Bash => "run.sh",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::Bash => "bash",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub read_only: bool,
    pub destructive: bool,
    pub network: bool,
    pub filesystem: bool,
    pub requires_credentials: bool,
}

#[derive(Debug, Clone)]
pub struct ToolRecord {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub language: Language,
    pub entrypoint: String,
    pub input_schema: serde_json::Value,
    pub capabilities: Capabilities,
    pub keywords: Vec<String>,
    pub commands: Vec<String>,
    pub examples: Vec<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
}

impl From<ToolRecord> for Tool {
    fn from(rec: ToolRecord) -> Self {
        Tool {
            id: rec.id,
            name: rec.name,
            description: rec.description,
            language: rec.language,
            entrypoint: rec.entrypoint,
            keywords: rec.keywords,
            commands: rec.commands,
            capabilities: rec.capabilities,
            input_schema: if rec.input_schema.is_null() {
                serde_json::json!({})
            } else {
                rec.input_schema
            },
            examples: rec.examples,
            timeout_seconds: rec.timeout_seconds,
            version: rec.version,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
            usage_count: rec.usage_count,
            last_used_at: rec.last_used_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub language: Language,
    pub entrypoint: String,
    pub keywords: Vec<String>,
    pub commands: Vec<String>,
    pub capabilities: Capabilities,
    pub input_schema: serde_json::Value,
    pub examples: Vec<serde_json::Value>,
    pub timeout_seconds: Option<u64>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewTool {
    pub name: String,
    pub description: String,
    pub language: Language,
    pub script: String,
    pub entrypoint: Option<String>,
    pub input_schema: serde_json::Value,
    pub keywords: Vec<String>,
    pub commands: Vec<String>,
    pub examples: Vec<serde_json::Value>,
    pub capabilities: Capabilities,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolYaml {
    pub name: String,
    pub description: String,
    pub language: Language,
    pub entrypoint: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default = "default_schema")]
    pub input_schema: serde_json::Value,
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

fn default_schema() -> serde_json::Value {
    serde_json::json!({})
}

impl ToolYaml {
    pub fn from_record(rec: &ToolRecord) -> Self {
        Self {
            name: rec.name.clone(),
            description: rec.description.clone(),
            language: rec.language,
            entrypoint: rec.entrypoint.clone(),
            keywords: rec.keywords.clone(),
            commands: rec.commands.clone(),
            capabilities: rec.capabilities.clone(),
            input_schema: if rec.input_schema.is_null() {
                serde_json::json!({})
            } else {
                rec.input_schema.clone()
            },
            examples: rec.examples.clone(),
            timeout_seconds: rec.timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSource {
    pub name: String,
    pub language: Language,
    pub entrypoint: String,
    pub script: String,
    pub metadata_yaml: String,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub description: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
    pub language: Language,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_helpers() {
        assert_eq!(Language::Python.interpreter(), "python3");
        assert_eq!(Language::Bash.interpreter(), "bash");
        assert_eq!(Language::Python.default_entrypoint(), "run.py");
        assert_eq!(Language::Bash.default_entrypoint(), "run.sh");
        assert_eq!(Language::Python.to_string(), "python");
        assert_eq!(serde_yml::to_string(&Language::Bash).unwrap().trim(), "bash");
    }

    #[test]
    fn capabilities_default_serde() {
        let caps: Capabilities = serde_json::from_str("{}").unwrap();
        assert!(!caps.destructive);
        let caps: Capabilities = serde_json::from_str(r#"{"read_only": true}"#).unwrap();
        assert!(caps.read_only);
    }

    #[test]
    fn tool_yaml_round_trip() {
        let yaml = ToolYaml {
            name: "t".into(),
            description: "d".into(),
            language: Language::Python,
            entrypoint: "run.py".into(),
            keywords: vec!["k".into()],
            commands: vec!["c".into()],
            capabilities: Capabilities::default(),
            input_schema: serde_json::json!({"type": "object"}),
            examples: vec![serde_json::json!({"a": 1})],
            timeout_seconds: Some(30),
        };
        let s = serde_yml::to_string(&yaml).unwrap();
        let back: ToolYaml = serde_yml::from_str(&s).unwrap();
        assert_eq!(back, yaml);
    }

    #[test]
    fn tool_yaml_from_record_maps_all_fields() {
        let rec = ToolRecord {
            id: 7,
            name: "some_tool".into(),
            description: "does things".into(),
            language: Language::Bash,
            entrypoint: "run.sh".into(),
            input_schema: serde_json::json!({"type": "object"}),
            capabilities: Capabilities::default(),
            keywords: vec!["kw".into()],
            commands: vec!["cmd".into()],
            examples: vec![serde_json::json!({"x": "a"})],
            timeout_seconds: Some(12),
            version: 3,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            usage_count: 5,
            last_used_at: None,
        };
        let yaml = ToolYaml::from_record(&rec);
        assert_eq!(yaml.name, "some_tool");
        assert_eq!(yaml.description, "does things");
        assert_eq!(yaml.language, Language::Bash);
        assert_eq!(yaml.entrypoint, "run.sh");
        assert_eq!(yaml.keywords, rec.keywords);
        assert_eq!(yaml.commands, rec.commands);
        assert_eq!(yaml.examples, rec.examples);
        assert_eq!(yaml.timeout_seconds, Some(12));

        let tool = Tool::from(rec);
        assert_eq!(tool.keywords, vec!["kw".to_string()]);
        assert_eq!(tool.timeout_seconds, Some(12));
    }
}
