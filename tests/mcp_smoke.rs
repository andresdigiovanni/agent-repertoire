use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn mcp_tools_list_and_search() {
    let rep = env!("CARGO_BIN_EXE_rep");
    let home = tempfile::tempdir().unwrap();

    let fixture = tempfile::tempdir().unwrap();
    std::fs::write(
        fixture.path().join("tool.yaml"),
        "name: smoke_echo\ndescription: Echo tool for MCP smoke test\nlanguage: bash\nentrypoint: run.sh\n",
    )
    .unwrap();
    std::fs::write(
        fixture.path().join("run.sh"),
        "#!/usr/bin/env bash\nread -r input\necho \"{\\\"echoed\\\": $input}\"\n",
    )
    .unwrap();
    let seeded = Command::new(rep)
        .args([
            "create",
            "--file",
            fixture.path().join("tool.yaml").to_str().unwrap(),
        ])
        .env("AGENT_REPERTOIRE_HOME", home.path())
        .output()
        .unwrap();
    assert!(
        seeded.status.success(),
        "seed failed: {}",
        String::from_utf8_lossy(&seeded.stderr)
    );

    let mut child = Command::new(rep)
        .args(["mcp"])
        .env("AGENT_REPERTOIRE_HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"t\",\"version\":\"0\"}}}}}}"
    )
    .unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}").unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}}").unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{{\"name\":\"search_tools\",\"arguments\":{{\"query\":\"smoke\"}}}}}}"
    )
    .unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{{\"name\":\"run_tool\",\"arguments\":{{\"name\":\"smoke_echo\",\"arguments\":{{\"x\":\"hi\"}}}}}}}}"
    )
    .unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{{\"name\":\"get_tool_source\",\"arguments\":{{\"name\":\"smoke_echo\"}}}}}}"
    )
    .unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    let mut saw_list = false;
    let mut saw_search = false;
    let mut saw_run = false;
    let mut saw_source = false;
    for line in reader.lines() {
        let line = line.unwrap();
        if line.contains("\"id\":2") && line.contains("search_tools") && line.contains("create_tool") {
            saw_list = true;
        }
        if line.contains("\"id\":3") && line.contains("smoke_echo") {
            saw_search = true;
        }
        if line.contains("\"id\":4") && line.contains("echoed") {
            saw_run = true;
        }
        if line.contains("\"id\":5") && line.contains("metadata_yaml") && line.contains("run.sh") {
            saw_source = true;
        }
        if saw_list && saw_search && saw_run && saw_source {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(saw_list, "tools/list response missing or incomplete: {}", "check server output");
    assert!(saw_search, "search_tools did not return smoke_echo");
    assert!(saw_run, "run_tool did not return the tool output");
    assert!(saw_source, "get_tool_source did not return metadata_yaml with entrypoint run.sh");
}

#[test]
fn mcp_input_schemas_have_no_boolean_property_schemas() {
    let rep = env!("CARGO_BIN_EXE_rep");
    let home = tempfile::tempdir().unwrap();

    let mut child = Command::new(rep)
        .args(["mcp"])
        .env("AGENT_REPERTOIRE_HOME", home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"t\",\"version\":\"0\"}}}}}}"
    )
    .unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}}").unwrap();
    writeln!(stdin, "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}}").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let stdout = child.stdout.take().unwrap();
    let mut listed: Option<serde_json::Value> = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.unwrap();
        if line.contains("\"id\":2") {
            let v: serde_json::Value =
                serde_json::from_str(&line).expect("tools/list response is valid JSON");
            listed = Some(v);
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    let v = listed.expect("no tools/list response");
    let tools = v["result"]["tools"].as_array().expect("tools array");
    let mut offenders = Vec::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or_default().to_string();
        if let Some(props) = tool["inputSchema"]["properties"].as_object() {
            for (key, schema) in props {
                if !schema.is_object() {
                    offenders.push(format!("{name}.{key}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "boolean property schemas are rejected by MCP TypeScript clients (zod): {offenders:?}"
    );
}
