# Creating a Reusable Tool

Use this guide when `Agent Repertoire` does not contain a suitable tool and the completed workflow appears stable, parameterizable, and likely to be reused.

The goal is to create a **small, deterministic, well-scoped tool** that hides operational complexity behind a clear interface.

## 1. Confirm that a tool is justified

Create a tool only when the workflow has reusable value.

Before creating one, verify:

* The workflow contains multiple meaningful operational steps or repeated CLI/API operations.
* The procedure is stable and deterministic.
* The required inputs can be expressed as named parameters.
* The workflow is likely to recur.
* Encapsulation reduces future effort, latency, complexity, or error rate.

Do not create a tool merely because a workflow is long. Long exploratory workflows are often better left as one-off procedures.

## 2. Define the interface first

Design the tool around the user's intent, not around the commands used internally.

Prefer:

```text
gcp_analyze_service_errors(
  service: string,
  duration?: string
)
```

over:

```text
run_gcloud(
  args: string
)
```

A good interface:

* Uses named parameters.
* Has clear parameter names.
* Has explicit required and optional inputs.
* Uses constrained values when possible.
* Avoids arbitrary free-form command strings.
* Hides implementation details from callers.

The caller should not need to know which Bash commands, CLI flags, APIs, or intermediate transformations the tool uses.

## 3. Choose a stable tool name

Use a descriptive, action-oriented name.

Prefer:

```text
gcp_analyze_service_errors
k8s_restart_deployment
aws_rotate_secret
```

Avoid:

```text
helper
run_commands
debug_script
misc_gcp
```

The name should describe the reusable operation and be understandable without inspecting the implementation.

## 4. Implement the workflow

Move the proven workflow into the tool.

Keep the implementation:

* Deterministic.
* Idempotent where practical.
* Explicit about failures.
* Free of unnecessary interactive prompts.
* Independent of local shell state when possible.
* Safe with respect to destructive operations.

Do not expose internal implementation details as parameters unless callers genuinely need to control them.

For example, if the workflow always queries the last 30 minutes by default, expose:

```text
duration?: string
```

rather than requiring callers to construct CLI flags.

## 5. Validate inputs

Validate parameters before performing operational work.

Examples:

* Required strings must be present.
* Enumerated values should be checked against the allowed set.
* Durations, paths, identifiers, and numeric limits should be validated.
* Reject ambiguous or unsafe input rather than guessing.

Prefer an explicit failure such as:

```text
Invalid duration: "abc". Expected a duration such as "30m" or "2h".
```

over silently changing the user's input.

## 6. Handle failures explicitly

A reusable tool should make failures understandable.

Return structured information when possible:

```text
{
  status: "error",
  error_type: "authentication",
  message: "...",
  retryable: false
}
```

Distinguish between:

* Invalid input.
* Authentication or authorization failures.
* Missing resources.
* External command failures.
* Timeouts.
* Empty or partial results.

Do not hide errors behind generic success responses.

## 7. Keep output useful and stable

Return information that helps the caller continue the task.

Prefer structured output such as:

```text
{
  service: "payments",
  duration: "30m",
  logs_analyzed: 1842,
  error_groups: [...]
}
```

Avoid returning raw CLI output when it can be converted into stable fields.

Do not include unnecessary implementation details, temporary files, shell commands, or verbose logs in the normal result.

## 8. Add examples

Document at least one realistic invocation.

Example:

```text
Tool: gcp_analyze_service_errors

Input:
{
  service: "payments",
  duration: "30m"
}

Output:
{
  logs_analyzed: 1842,
  error_groups: [...]
}
```

Examples should demonstrate the intended abstraction, not the internal implementation.

## 9. Test the tool

Before adding the tool to the repertoire, test:

### Happy path

A normal invocation produces the expected result.

### Required parameters

Missing required inputs fail clearly.

### Invalid parameters

Malformed or unsupported values are rejected.

### Empty results

The tool behaves correctly when the underlying system has no matching data.

### External failures

Authentication failures, unavailable services, command failures, and timeouts are surfaced correctly.

### Repeated execution

Running the tool twice with the same inputs behaves predictably.

### Boundary cases

Test relevant limits such as empty strings, very large durations, nonexistent resources, or unusual identifiers.

## 10. Review the abstraction

Before registering the tool, ask:

* Would another agent understand what this tool does from its name and schema?
* Can the tool be reused without knowing its implementation?
* Are the parameters stable?
* Is the interface smaller than the workflow it replaces?
* Does the tool remove meaningful operational complexity?
* Would we still want this tool after the immediate task is forgotten?

If the answer is no, do not create the tool.

## 11. Register the tool

After implementation and validation:

1. Register the tool through the `create_tool` MCP tool.
2. Provide its name, description, parameter schema, examples, and timeout.
3. Set `timeout_seconds` high enough to cover the **worst-case runtime** (items to process x per-item cost, network latency, retries) — not the happy-path time. If the worst case exceeds the MCP client request timeout, document out-of-band execution in the tool description.
4. Make the description specific enough to be discoverable through `search_tools`.
5. Verify that `search_tools` can find it using the natural language description of the task.
6. Run the registered tool through the normal `search_tools` → `inspect_tool` → `run_tool` workflow.

The final test is not only "does the script work?" but also:

> Can another agent discover and use it correctly without knowing that the tool exists?

## 12. Verify the registered tool's output

Registration is not the last step. After registering:

1. Execute the tool with realistic arguments via `run_tool` (or out-of-band if its runtime exceeds the client timeout).
2. Inspect the actual result: check returned fields are populated and sensible, and if the tool writes files, open them and confirm their content is correct and complete.
3. Do not accept "exit code 0" or a non-error status as proof of success — a tool that runs but returns wrong/incomplete data is broken.
4. If the output is wrong, follow the *Updating an existing tool* flow in SKILL.md (retrieve source, fix, republish) before reporting the task complete.

## Completion checklist

Before considering a new tool complete:

* [ ] Reuse was ruled out by searching the repertoire.
* [ ] The workflow is stable and reusable.
* [ ] The tool has a clear, action-oriented name.
* [ ] Inputs are named and parameterized.
* [ ] Inputs are validated.
* [ ] Implementation details are hidden behind the interface.
* [ ] Failures are explicit and actionable.
* [ ] Output is structured and stable.
* [ ] At least one realistic example exists.
* [ ] Happy path and failure cases were tested.
* [ ] `timeout_seconds` covers the worst-case runtime.
* [ ] The registered tool is discoverable through `search_tools`.
* [ ] The registered tool was executed after registration and its output was **verified against expectations**.
