use crate::models::{Capabilities, Language, NewTool};
use crate::registry::Registry;
use crate::search;
use crate::RepError;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerConfig};
use rmcp::service::serve_server;
use rmcp::schemars::JsonSchema;
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use serde::Deserialize;


#[derive(Debug, Deserialize, JsonSchema)]
struct SearchToolsRequest {
    query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct InspectToolRequest {
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunToolRequest {
    name: String,
    #[schemars(schema_with = "any_value_schema")]
    arguments: serde_json::Value,
    timeout_seconds: Option<u64>,
}

fn any_value_schema(_generator: &mut rmcp::schemars::SchemaGenerator) -> rmcp::schemars::Schema {
    let value = serde_json::json!({
        "anyOf": [
            {"type": "object", "additionalProperties": true},
            {"type": "array"},
            {"type": "string"},
            {"type": "number"},
            {"type": "boolean"},
            {"type": "null"}
        ]
    });
    let map = match value {
        serde_json::Value::Object(m) => m,
        _ => unreachable!(),
    };
    rmcp::schemars::Schema::from(map)
}

mod any_value {
    use rmcp::schemars::Schema;
    use rmcp::schemars::SchemaGenerator;
    use rmcp::schemars::JsonSchema;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Debug, Default, Clone)]
    pub struct AnyValue(pub Option<serde_json::Value>);

    impl JsonSchema for AnyValue {
        fn schema_name() -> std::borrow::Cow<'static, str> {
            "AnyValue".into()
        }

        fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
            super::any_value_schema(_generator)
        }
    }

    impl Serialize for AnyValue {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match &self.0 {
                Some(v) => v.serialize(serializer),
                None => serializer.serialize_none(),
            }
        }
    }

    impl<'de> Deserialize<'de> for AnyValue {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
            Ok(AnyValue(opt))
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateToolRequest {
    name: String,
    description: String,
    language: String,
    script: String,
    entrypoint: Option<String>,
    input_schema: Option<any_value::AnyValue>,
    keywords: Option<Vec<String>>,
    commands: Option<Vec<String>>,
    examples: Option<Vec<serde_json::Value>>,
    capabilities: Option<any_value::AnyValue>,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetToolSourceRequest {
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateToolRequest {
    name: String,
    script: Option<String>,
    description: Option<String>,
    keywords: Option<Vec<String>>,
    commands: Option<Vec<String>>,
    input_schema: Option<any_value::AnyValue>,
    examples: Option<Vec<serde_json::Value>>,
    capabilities: Option<any_value::AnyValue>,
    timeout_seconds: Option<u64>,
}

pub struct RepertoireServer {
    registry: Registry,
}

fn tool_error(e: RepError) -> std::result::Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::error(vec![ContentBlock::text(e.to_string())]))
}

fn coerce_stringified_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => {
            serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))
        }
        other => other,
    }
}

fn json_ok(value: serde_json::Value) -> std::result::Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(value);
    Ok(result)
}

#[tool_router]
impl RepertoireServer {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    #[tool(description = "Search the Agent Repertoire for reusable tools matching a free-text query. Returns scored candidates; never executes anything.")]
    async fn search_tools(
        &self,
        params: Parameters<SearchToolsRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        match self
            .registry
            .search(&params.0.query, search::DEFAULT_LIMIT)
        {
            Ok(hits) => json_ok(serde_json::json!({ "tools": hits })),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Retrieve the complete definition of a tool (parameters, capabilities, examples). Does not execute it.")]
    async fn inspect_tool(
        &self,
        params: Parameters<InspectToolRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        match self.registry.inspect(&params.0.name) {
            Ok(tool) => json_ok(serde_json::to_value(tool).unwrap_or_default()),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Execute a registered tool. Arguments are validated against the tool's input schema and passed as JSON on the subprocess stdin. Returns exit_code, stdout, stderr, duration_ms, timed_out, truncated.")]
    async fn run_tool(
        &self,
        params: Parameters<RunToolRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let req = params.0;
        let arguments = coerce_stringified_json(req.arguments);
        match self.registry.run(&req.name, &arguments, req.timeout_seconds) {
            Ok(outcome) => json_ok(serde_json::to_value(outcome).unwrap_or_default()),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Create (publish) a new reusable tool. `script` is the full source code; `language` is \"python\" or \"bash\". `capabilities` is an optional object with read_only, destructive, network, filesystem, requires_credentials flags.")]
    async fn create_tool(
        &self,
        params: Parameters<CreateToolRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let req = params.0;
        let language = match req.language.as_str() {
            "python" => Language::Python,
            "bash" => Language::Bash,
            other => {
                return tool_error(RepError::InvalidTool(
                    crate::InvalidToolIssues(vec![format!(
                        "unsupported language '{}'; expected \"python\" or \"bash\"",
                        other
                    )]),
                ))
            }
        };
        let capabilities = match req.capabilities.and_then(|c| c.0).map(coerce_stringified_json) {
            Some(c) => match serde_json::from_value::<Capabilities>(c) {
                Ok(caps) => caps,
                Err(e) => {
                    return tool_error(RepError::InvalidTool(crate::InvalidToolIssues(vec![format!(
                        "invalid capabilities: {e}"
                    )])))
                }
            },
            None => Capabilities::default(),
        };
        let spec = NewTool {
            name: req.name,
            description: req.description,
            language,
            script: req.script,
            entrypoint: req.entrypoint,
            input_schema: req
                .input_schema
                .and_then(|s| s.0)
                .map(coerce_stringified_json)
                .unwrap_or_else(|| serde_json::json!({})),
            keywords: req.keywords.unwrap_or_default(),
            commands: req.commands.unwrap_or_default(),
            examples: req
                .examples
                .map(|xs| xs.into_iter().map(coerce_stringified_json).collect())
                .unwrap_or_default(),
            capabilities,
            timeout_seconds: req.timeout_seconds,
        };
        match self.registry.create_tool(spec) {
            Ok(tool) => json_ok(serde_json::to_value(tool).unwrap_or_default()),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Retrieve the full source script and metadata (YAML generated from the catalog) of a registered tool, plus its current version. Does not execute it.")]
    async fn get_tool_source(
        &self,
        params: Parameters<GetToolSourceRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        match self.registry.get_tool_source(&params.0.name) {
            Ok(src) => json_ok(serde_json::to_value(src).unwrap_or_default()),
            Err(e) => tool_error(e),
        }
    }

    #[tool(description = "Update a registered tool in place: replace the script and/or metadata fields. Only provided fields change. Increments version and preserves usage statistics. Validate the new behavior by running the tool afterwards.")]
    async fn update_tool(
        &self,
        params: Parameters<UpdateToolRequest>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let req = params.0;
        let mut updated: Vec<&str> = Vec::new();
        if req.script.is_some() { updated.push("script"); }
        if req.description.is_some() { updated.push("description"); }
        if req.keywords.is_some() { updated.push("keywords"); }
        if req.commands.is_some() { updated.push("commands"); }
        if req.input_schema.is_some() { updated.push("input_schema"); }
        if req.examples.is_some() { updated.push("examples"); }
        if req.capabilities.is_some() { updated.push("capabilities"); }
        if req.timeout_seconds.is_some() { updated.push("timeout_seconds"); }
        let capabilities = match req.capabilities.and_then(|c| c.0).map(coerce_stringified_json) {
            Some(c) => match serde_json::from_value::<Capabilities>(c) {
                Ok(caps) => Some(caps),
                Err(e) => {
                    return tool_error(RepError::InvalidTool(crate::InvalidToolIssues(vec![format!(
                        "invalid capabilities: {e}"
                    )])))
                }
            },
            None => None,
        };
        let update = crate::registry::ToolUpdate {
            script: req.script,
            description: req.description,
            keywords: req.keywords,
            commands: req.commands,
            input_schema: req.input_schema.and_then(|s| s.0).map(coerce_stringified_json),
            examples: req.examples.map(|xs| xs.into_iter().map(coerce_stringified_json).collect()),
            capabilities,
            timeout_seconds: req.timeout_seconds,
        };
        match self.registry.update_tool(&req.name, update) {
            Ok(tool) => json_ok(serde_json::json!({
                "status": "ok",
                "name": tool.name,
                "version": tool.version,
                "updated": updated,
            })),
            Err(e) => tool_error(e),
        }
    }
}

#[tool_handler]
impl ServerHandler for RepertoireServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Agent Repertoire: search for an existing tool before solving operational tasks; inspect it to see parameters; run it with arguments; create a new tool only for stable, parameterizable, multi-step procedures.",
            )
    }
}

pub fn serve() -> crate::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RepError::Spawn(format!("tokio: {}", e)))?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async {
        let registry = Registry::open(None)?;
        let server = RepertoireServer::new(registry);
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let running = serve_server(server, transport)
            .await
            .map_err(|e| RepError::Spawn(format!("mcp: {}", e)))?;
        let _ = running.waiting().await;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn coerces_stringified_object() {
        let value = json!(r#"{"type": "object", "properties": {"x": 1}}"#);
        assert_eq!(coerce_stringified_json(value), json!({"type": "object", "properties": {"x": 1}}));
    }

    #[test]
    fn coerces_stringified_array() {
        let value = json!("[1, 2, 3]");
        assert_eq!(coerce_stringified_json(value), json!([1, 2, 3]));
    }

    #[test]
    fn keeps_plain_strings() {
        let value = json!("hello world");
        assert_eq!(coerce_stringified_json(value), json!("hello world"));
    }

    #[test]
    fn keeps_non_string_values() {
        assert_eq!(coerce_stringified_json(json!({"a": 1})), json!({"a": 1}));
        assert_eq!(coerce_stringified_json(json!(5)), json!(5));
        assert_eq!(coerce_stringified_json(json!(null)), json!(null));
    }

    #[test]
    fn any_value_schema_is_not_required_and_not_boolean() {
        let schema = rmcp::schemars::schema_for!(CreateToolRequest);
        let required = schema.get("required").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let required: Vec<String> = required.into_iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        assert_eq!(
            required,
            vec!["name".to_string(), "description".to_string(), "language".to_string(), "script".to_string()],
            "create_tool must only require truly required fields"
        );
        let properties = schema.get("properties").and_then(|v| v.as_object()).expect("properties");
        for key in ["input_schema", "capabilities"] {
            let prop = properties.get(key).unwrap_or_else(|| panic!("missing property {key}"));
            assert!(prop.is_object(), "{key} schema must be a JSON object (zod rejects booleans)");
        }
    }
}
