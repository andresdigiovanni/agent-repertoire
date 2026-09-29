---

name: agent-repertoire
description: Call search_tools FIRST, before writing any ad-hoc script (bash, node, python, heredoc) or chaining several commands to compute a result — a registered tool from an earlier session may already do it. Covers any domain: code analysis (call graphs, dependency diagrams, per-function counts or metrics), file or report generation, data extraction, logs and incidents, cloud/infra operations. Also use when a task repeats or varies earlier-session work, and to extend a near-fit tool instead of reimplementing it. Not for plain lookups, explanations or hand edits.
---

# Agent Repertoire

Agent Repertoire is a catalog of reusable operational tools.

Use it to **discover and reuse existing tools before rebuilding operational workflows manually**. When no suitable tool exists, use the repertoire's creation process to turn stable, recurring workflows into reusable tools.

## When to use

The trigger is the **shape of the work, not its domain**: you are about to compute a result mechanically — write a script, parse an AST, walk files, call a CLI or API several times, post-process output. Past sessions may have already turned that procedure into a tool, and the tool encodes conventions (definitions, filters, edge cases) that a fresh reimplementation silently drifts from.

Search the repertoire when any of these holds:

* You are about to write a throwaway script or a multi-line heredoc to produce an answer.
* The task repeats or varies something done in a previous session ("update the diagram", "rerun the analysis with a different filter", "same report for another service").
* The task is operational: logs, incidents, deployments, Kubernetes/cloud resources, secrets, backups.
* You would otherwise answer by chaining several searches or reads into a computed result (counts, rankings, graphs).

Other code tools (codegraph, grep, Read) don't replace the search: they help you do the work by hand, while a registered tool may already do it mechanically.

### Examples

Search first for requests such as:

* "Update the function-dependency diagram, keeping only functions with more than 2 callers."
* "Rank the functions in this file by how many callers they have."
* "Why is the payments service returning 500s?"
* "Find which pods are crashlooping and summarize the errors."
* "Run our standard database backup verification."

Do **not** search for self-contained lookups or edits that no script would express, such as:

* `pwd`, `ls`, `cat file.txt`, `git status`, a single grep.
* Explaining what a function does.
* Editing code or prose by hand.

When uncertain, **search — it is one call — rather than assuming no tool exists**.

## Reusing an existing tool

For a task that falls within the scope above:

1. **Search**

   * Call `search_tools` with a short, task-focused query.
   * Use the user's operational intent rather than implementation details.
   * Examples: `gcp cloud run errors`, `k8s crashloop`, `aws rotate secret`.

2. **Inspect**

   * For each plausible match, call `inspect_tool`.
   * Check the tool's purpose, parameters, examples, and timeout.
   * Prefer the smallest tool that fully covers the requested operation.

3. **Run**

   * If a suitable tool exists, call `run_tool`.
   * Use explicit, deterministic, named arguments.
   * Do not manually recreate a workflow already provided by a suitable tool.

4. **Complete uncovered work**

   * If the tool covers only part of the task, use it for that portion.
   * Perform only the uncovered steps directly.
   * If the tool almost fits — the same procedure with a different filter, threshold or output — add a parameter with `update_tool` (see *Updating an existing tool*) instead of writing a parallel script.

## Timeouts and long-running tools

Two independent timeouts apply when running a tool through MCP:

* **Client request timeout**: configured in opencode under `mcp.<server>.timeout` (milliseconds, default 5000). It caps every MCP call. When it fires you get `MCP error -32001: Request timed out`, but the server-side subprocess **keeps running** and may still finish its work (write files, update state) after the client gave up.
* **Server subprocess timeout**: `timeout_seconds` declared in the tool (visible via `inspect_tool`), overridable per call through `run_tool`'s `timeout_seconds` argument. It only limits the subprocess — it does **not** extend the client request timeout.

### Setup requirement — recheck on every install

Since `rep` v0.1, `rep install` registers the `agent-repertoire` MCP server with `timeout: 300000` automatically for opencode, Claude, and codex — creating or patching the entry as needed. Config files containing comments are never rewritten; a manual snippet is printed instead. Still verify after installing on a new machine that the opencode config contains:

```jsonc
{
  "mcp": {
    "agent-repertoire": {
      "type": "local",
      "command": ["<path-to-rep>", "mcp"],
      "enabled": true,
      "timeout": 300000
    }
  }
}
```

Restart opencode for config changes to apply. The default of 5000 ms is too short for network-bound tools. If `timeout` is missing or below 300000 (5 minutes), fix it and restart opencode — until then, long-running tools must be executed out-of-band (see rule 2 below).

Rules:

1. Before running a tool that does network work or iterates over many items, check its `timeout_seconds` and estimate the runtime (e.g., N items x per-item cost). If the estimated runtime exceeds the configured client timeout, do not call `run_tool` expecting it to succeed.
2. For long-running tools, either raise `mcp.<server>.timeout` in opencode config (requires an opencode restart), or run the tool out-of-band: pipe the JSON arguments into the script directly, e.g.
   `echo '{"project_path": "..."}' | python3 ~/.agent-repertoire/tools/<name>/<entrypoint>.py`
3. After a `Request timed out`, check for side effects (report files, changed state) before re-running — a previous attempt may have completed in the background.
4. When creating a tool, set its `timeout_seconds` to comfortably cover the worst-case runtime, and keep the per-item cost documented in the description if the runtime scales with input size.

## When no suitable tool exists

If `search_tools` finds no suitable tool:

1. Perform the task directly using the appropriate commands or APIs.
2. Do not create a tool merely because the task was multi-step.
3. Before reporting the task complete, evaluate the workflow for registration. The strongest signal: you already wrote a working, parameterizable script for it. Other signals: multiple successful API/CLI operations, a stable input set, and likely recurrence.
4. If it is stable, parameterizable, and likely to recur, read:
   `references/creating-tools.md`
5. Follow that guide to design, implement, validate, and register the new tool.
6. If you decide not to register, state that decision and its reason in one sentence when reporting the task.

## When to create a tool

A new tool is justified when the completed workflow:

* Contains multiple meaningful operational steps.
* Uses multiple Bash, CLI, or API operations.
* Is stable rather than exploratory or one-off.
* Has a clear, parameterizable interface.
* Is likely to recur.
* Provides meaningful savings in effort, latency, complexity, or error rate when encapsulated.

Prefer creating a tool **after the workflow is understood and proven**, rather than abstracting an evolving investigation.

## Do not create tools for

Do not create tools for:

* Trivial commands.
* Single-line greps or simple pipelines.
* One-off experiments.
* Exceptional investigations unlikely to recur.
* Workflows dominated by arbitrary free-form text.
* Procedures whose inputs or behavior are too unstable to define.
* Thin wrappers that provide little value over the underlying command.

## Priority

Follow this decision order:

> **Trivial operation → execute directly.**
> **Non-trivial operational workflow → search Agent Repertoire first.**
> **Suitable tool exists → inspect and reuse it.**
> **No suitable tool → perform the task directly.**
> **Stable and reusable workflow → consider creating a tool.**

The goal is not to maximize the number of tools.

> **Maximize the reusable value of the repertoire, not its size.**

Every tool introduces maintenance cost. Prefer a small set of well-defined tools with stable interfaces over many narrow or speculative abstractions.

## Tool creation guidance

When creating a new tool, read:

`references/creating-tools.md`

**Delegate the implementation to a sub-agent.** Once you have decided to create a tool and know its interface, dispatch a general-purpose sub-agent with the full requirements (purpose, parameters, schemas, test cases). The sub-agent implements the script, tests it, and returns the script plus test evidence. The main agent reviews the result, registers the tool, and verifies it. This keeps the exploration-heavy implementation work out of the main context.

The new tool should have:

* A descriptive, action-oriented name.
* A small, explicit parameter schema.
* Named inputs instead of positional or arbitrary free-form arguments.
* Input validation.
* Predictable behavior and error handling.
* Stable, useful output.
* Realistic usage examples.
* Tests covering normal and failure cases.
* A description that makes the tool discoverable through `search_tools`.

**After registering a tool, always test it and verify the output.** Run the registered tool with realistic arguments and inspect the actual result — confirm the returned data (or written report) is correct and complete, not merely that the exit code was 0. A tool is not done until its verified output matches the expectation.

## Updating an existing tool

When a tool is used and the result is wrong, incomplete, or suspected to be broken, fix the tool instead of working around it manually:

1. **Retrieve the current script** with `get_tool_source` (or read `~/.agent-repertoire/tools/<name>/<entrypoint>` directly from disk).
2. **Diagnose** the root cause before editing (use systematic debugging; check for environment, network, or scale changes since the tool last worked).
3. **Edit and test the script** out-of-band (run it directly with sample input) until it behaves correctly.
4. **Publish the update** with `update_tool`, which replaces the script, increments the tool's version, and preserves usage statistics. As a fallback when that tool is unavailable: edit the script in place under `~/.agent-repertoire/tools/<name>/`, then verify it by running it directly with sample input.
5. **Re-run the registered tool** through `run_tool` and verify the output again.

## Example: reusing an existing tool

**User:** "Why is payments returning 500 errors?"

1. `search_tools("gcp cloud run 500 errors")`
2. `inspect_tool("gcp_analyze_service_errors")`
3. `run_tool("gcp_analyze_service_errors", { service: "payments", duration: "30m" })`
4. Analyze the structured result and answer the user.

If the same request occurs later, search the repertoire again and reuse the registered tool.

## Example: creating a new tool

**User:** "Investigate this issue using our standard sequence of five CLI commands."

1. Search the repertoire.
2. If no suitable tool exists, perform the investigation directly.
3. Determine whether the sequence is stable, parameterizable, and likely to recur.
4. If justified, read `references/creating-tools.md`.
5. Implement, validate, and register the new tool.
