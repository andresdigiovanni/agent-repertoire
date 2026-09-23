# Agent Repertoire

A personal repertoire of reusable operational tools for AI agents — exposed to agents over MCP and to humans over a CLI, backed by SQLite/FTS5 and plain files.

## What is it?

Agents routinely re-execute expensive multi-step workflows (`gcloud logging read` × 3 + `jq` + `python` …) instead of reusing what already worked. Agent Repertoire lets an agent **discover, inspect, execute, and create** parameterized tools once, then reuse them forever:

- Tool metadata lives in `~/.agent-repertoire/repertoire.db` (SQLite, FTS-indexed); scripts live in `~/.agent-repertoire/tools/<name>/`.
- The MCP server exposes six tools (`search_tools`, `inspect_tool`, `run_tool`, `create_tool`, `get_tool_source`, `update_tool`), so learned tools never pollute the agent's tool list.
- Tool arguments are validated against a JSON Schema before execution and passed as JSON on stdin — no shell, no string concatenation.
- Identical behavior from the CLI (`rep …`) and the MCP server (`rep mcp`).

## Install

One line, no Rust, no sudo:

```bash
curl -fsSL https://raw.githubusercontent.com/andresdigiovanni/agent-repertoire/main/install.sh | bash
```

The script detects your platform (Linux x86_64/arm64, macOS Intel/Apple Silicon), downloads the matching binary from GitHub Releases, verifies it against the published `SHA256SUMS`, and installs it to `~/.local/bin/rep`. If that directory is not on your `PATH`, the installer prints the exact line to add to your shell profile.

On Windows, download `rep-v<version>-x86_64-pc-windows-msvc.zip` from the [releases page](https://github.com/andresdigiovanni/agent-repertoire/releases/latest) and extract `rep.exe` to a directory on your `PATH`.

To install a specific version instead of the latest:

```bash
curl -fsSL https://raw.githubusercontent.com/andresdigiovanni/agent-repertoire/main/install.sh | bash -s -- --version v0.1.0
```

### Requirements

- `bash` and `curl` (or `wget`) to run the installer.
- `python3` and `bash` on `PATH` — stored tools run as Python or Bash subprocesses.
- One of Claude Code, OpenCode, or Codex, if you want your agent to use the repertoire.

## Update

Re-run the same one-liner — it upgrades `~/.local/bin/rep` in place. To go back to a previous release, pass `--version vX.Y.Z`.

## Uninstall

1. Remove the Skill and the MCP entry from every configured agent:

```bash
rep uninstall
```

This deletes the `agent-repertoire` skill directory from Claude Code, OpenCode, and Codex, and strips the `agent-repertoire` block from each agent's MCP config (`~/.claude.json`, `~/.config/opencode/opencode.json{,.c}`, `~/.codex/config.toml`). Re-running is safe; anything already absent is skipped silently (`nothing to uninstall` when nothing is left) and the command exits 0.

2. To also delete the data directory (`~/.agent-repertoire/`, containing every stored tool), pass `--all`:

```bash
rep uninstall --all      # interactive confirmation
rep uninstall --all --yes   # non-interactive / scripts
```

Without `--all`, the data directory is untouched.

3. Remove the binary:

```bash
rm ~/.local/bin/rep
```

## What it installs and what it modifies

| Path | What | Created by |
|---|---|---|
| `~/.local/bin/rep` | The binary | `install.sh` |
| `~/.agent-repertoire/` | `repertoire.db` (SQLite + FTS5) and `tools/<name>/` (tool scripts) | first `rep` command |
| `~/.claude/skills/agent-repertoire/`, `~/.config/opencode/skills/agent-repertoire/`, `~/.codex/skills/agent-repertoire/` | The Skill (teaches your agent when to reach for the repertoire) | `rep install --target …` |
| `~/.claude.json`, `~/.config/opencode/opencode.json{,.c}`, `~/.codex/config.toml` | One MCP server entry per agent | `rep install --target …` |

The data root can be relocated with the `AGENT_REPERTOIRE_HOME` environment variable (useful for tests/CI).

## Set up your agent

Two pieces give an agent full access: registering the MCP server (so it can search, inspect, run, and create tools) and installing the Skill (so it knows when to do so). `rep install` does both for one agent; after configuring, restart your agent session.

```bash
rep install --target claude     # or: opencode, codex
rep install --all               # all three
```

Bare `rep install` (no flags) opens an interactive multi-select of the supported providers. Without a terminal (pipes, scripts, CI) it refuses to guess and exits with an error: pass `--target <claude|opencode|codex>` or `--all`.

Installing the Skill overwrites the skill directory (`SKILL.md` + `references/`) with a warning.

Prefer to configure by hand? Register the MCP server yourself:

### Claude Code

Add to `~/.claude.json` (user scope) or a project `.mcp.json`:

```json
{
  "mcpServers": {
    "agent-repertoire": {
      "command": "rep",
      "args": ["mcp"]
    }
  }
}
```

(If `~/.local/bin` is not on your agent's `PATH`, use `~/.local/bin/rep` as the command.)

### OpenCode

Merge into `~/.config/opencode/opencode.json` (or `opencode.jsonc`; respects `XDG_CONFIG_HOME`):

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "agent-repertoire": {
      "type": "local",
      "command": ["rep", "mcp"],
      "enabled": true
    }
  }
}
```

### Codex

Add to `~/.codex/config.toml` (or a project `.codex/config.toml` in trusted projects):

```toml
[mcp_servers.agent-repertoire]
command = "rep"
args = ["mcp"]
```

You can also register the server from the CLI: `codex mcp add agent-repertoire -- rep mcp`.

After restarting your agent, the six `agent-repertoire` tools should be available (check with `/mcp` where supported). The MCP server speaks stdio; logs go to stderr only, stdout is reserved for the protocol.

## Quickstart

```bash
# 1. Create a tool (definition + script)
mkdir -p my-tool && cd my-tool
cat > run.sh <<'EOF'
#!/usr/bin/env bash
read -r args
echo "{\"summary\": \"processed\", \"args\": $args}"
EOF
cat > tool.yaml <<'EOF'
name: my_first_tool
description: Echoes back whatever arguments it receives
language: bash
entrypoint: run.sh
keywords:
  - example
EOF
rep create --file tool.yaml

# 2. Find and run it
rep search "example"
rep run my_first_tool x=1
rep list
rep inspect my_first_tool
```

Agents (or you) can also create tools through the MCP `create_tool` call — see `docs/prd.md`.

## Development

```bash
git clone https://github.com/andresdigiovanni/agent-repertoire
cd agent-repertoire
cargo test                    # full suite
cargo install --path .        # installs to ~/.cargo/bin
```

- Rust 1.85+ with `cargo` is only needed for building from source — end users just need the [Install](#install) one-liner.
- Releases run through the manual `Release` GitHub Actions workflow; the process is documented in [`AGENTS.md`](AGENTS.md).
- Documentation: product requirements in [`docs/prd.md`](docs/prd.md), design specs in [`docs/superpowers/specs/`](docs/superpowers/specs/), implementation plans in [`docs/superpowers/plans/`](docs/superpowers/plans/).
