use crate::models::NewTool;
use crate::{InvalidArgumentIssues, InvalidToolIssues, RepError, Result};

pub fn validate_name(name: &str) -> bool {
    if !(3..=64).contains(&name.len()) {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn validate_entrypoint(entrypoint: &str) -> bool {
    !entrypoint.is_empty()
        && !entrypoint.contains('/')
        && !entrypoint.contains('\\')
        && entrypoint != "."
        && entrypoint != ".."
}

pub fn compile_input_schema(schema: &serde_json::Value) -> std::result::Result<(), String> {
    let obj = schema
        .as_object()
        .ok_or_else(|| "schema must be a JSON object".to_string())?;
    if obj.is_empty() {
        return Ok(());
    }
    if let Some(t) = obj.get("type") {
        if t.as_str() != Some("object") {
            return Err("schema root type must be \"object\"".to_string());
        }
    }
    match jsonschema::validator_for(schema) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("schema failed to compile: {}", e)),
    }
}

pub fn validate_arguments(schema: &serde_json::Value, args: &serde_json::Value) -> Result<()> {
    if !args.is_object() {
        return Err(RepError::InvalidArguments(InvalidArgumentIssues(vec![
            "arguments must be a JSON object".to_string(),
        ])));
    }
    let schema_obj = schema.as_object().ok_or_else(|| {
        RepError::InvalidArguments(InvalidArgumentIssues(vec![
            "input_schema must be a JSON object".to_string(),
        ]))
    })?;
    if schema_obj.is_empty() {
        return Ok(());
    }
    let validator = jsonschema::validator_for(schema).map_err(|e| {
        RepError::InvalidArguments(InvalidArgumentIssues(vec![format!(
            "input_schema failed to compile: {}",
            e
        )]))
    })?;
    let errors: Vec<String> = validator
        .iter_errors(args)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(RepError::InvalidArguments(InvalidArgumentIssues(errors)))
    }
}

pub fn validate_examples(
    examples: &[serde_json::Value],
    schema: &serde_json::Value,
) -> std::result::Result<(), String> {
    for (i, example) in examples.iter().enumerate() {
        validate_arguments(schema, example).map_err(|e| format!("example {}: {}", i + 1, e))?;
    }
    Ok(())
}

pub fn validate_new_tool(spec: &NewTool) -> Result<()> {
    let mut issues = Vec::new();
    if !validate_name(&spec.name) {
        issues.push(format!(
            "name '{}' must match ^[a-z][a-z0-9_]{{2,63}}$",
            spec.name
        ));
    }
    if spec.description.trim().is_empty() {
        issues.push("description must not be empty".to_string());
    }
    if spec.script.trim().is_empty() {
        issues.push("script must not be empty".to_string());
    }
    match &spec.entrypoint {
        Some(e) if !validate_entrypoint(e) => {
            issues.push(format!("entrypoint '{}' must be a bare file name", e))
        }
        _ => {}
    }
    if let Err(e) = compile_input_schema(&spec.input_schema) {
        issues.push(format!("input_schema: {}", e));
    }
    if let Err(e) = validate_examples(&spec.examples, &spec.input_schema) {
        issues.push(format!("examples: {}", e));
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(RepError::InvalidTool(InvalidToolIssues(issues)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Capabilities, Language, NewTool};

    fn spec(name: &str) -> NewTool {
        NewTool {
            name: name.into(),
            description: "d".into(),
            language: Language::Bash,
            script: "echo hi".into(),
            entrypoint: None,
            input_schema: serde_json::json!({}),
            keywords: vec![],
            commands: vec![],
            examples: vec![],
            capabilities: Capabilities::default(),
            timeout_seconds: None,
        }
    }

    #[test]
    fn names() {
        assert!(validate_name("gcp_analyze_service_errors"));
        assert!(!validate_name("Run"));
        assert!(!validate_name("1abc"));
        assert!(!validate_name("ab"));
        assert!(!validate_name("has-dash"));
        assert!(!validate_name("a".repeat(65).as_str()));
    }

    #[test]
    fn entrypoints() {
        assert!(validate_entrypoint("run.py"));
        assert!(!validate_entrypoint("a/b"));
        assert!(!validate_entrypoint(".."));
        assert!(!validate_entrypoint(""));
        assert!(!validate_entrypoint("."));
        assert!(!validate_entrypoint("\\"));
    }

    #[test]
    fn input_schema_root_rules() {
        assert!(compile_input_schema(&serde_json::json!({})).is_ok());
        assert!(compile_input_schema(&serde_json::json!({"type": "object"})).is_ok());
        assert!(compile_input_schema(&serde_json::json!({"type": "object", "properties": {"x": {"type": "string"}}})).is_ok());
        assert!(compile_input_schema(&serde_json::json!({"type": "array"})).is_err());
        assert!(compile_input_schema(&serde_json::json!(5)).is_err());
    }

    #[test]
    fn arguments_validated() {
        let schema = serde_json::json!({"type": "object", "properties": {"x": {"type": "string"}}, "required": ["x"]});
        assert!(validate_arguments(&schema, &serde_json::json!({"x": "a"})).is_ok());
        let err = validate_arguments(&schema, &serde_json::json!({"x": 5})).unwrap_err();
        assert!(err.to_string().contains("x"));
        let err = validate_arguments(&schema, &serde_json::json!({})).unwrap_err();
        assert!(err.to_string().contains("x"));
        assert!(validate_arguments(&serde_json::json!({}), &serde_json::json!({"anything": 1})).is_ok());
        assert!(validate_arguments(&serde_json::json!({}), &serde_json::json!([1])).is_err());
    }

    #[test]
    fn examples_validated() {
        let schema = serde_json::json!({"type": "object", "properties": {"x": {"type": "string"}}});
        assert!(validate_examples(&[serde_json::json!({"x": "a"})], &schema).is_ok());
        assert!(validate_examples(&[serde_json::json!({"x": 1})], &schema).is_err());
    }

    #[test]
    fn new_tool_aggregates_issues() {
        let mut s = spec("Bad Name");
        s.entrypoint = Some("../evil".into());
        let msg = validate_new_tool(&s).unwrap_err().to_string();
        assert!(msg.contains("name"));
        assert!(msg.contains("entrypoint"));
        assert!(validate_new_tool(&spec("good_tool")).is_ok());
        let mut s = spec("empty_script");
        s.script = String::new();
        assert!(validate_new_tool(&s).is_err());
    }
}
