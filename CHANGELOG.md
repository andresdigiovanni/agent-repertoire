# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Improve SKILL.md.

## [0.1.0] - 2026-09-29

### Changed

- Improve SKILL.md.

## [0.0.0] - 2026-09-23

### Added

- `rep` CLI and MCP server: a searchable repertoire of reusable agent tools
  backed by SQLite/FTS5 (`search`, `inspect`, `run`, `create`, `update`,
  `source`, `list`, `install`, `uninstall`).
- Tools run as isolated Python or Bash subprocesses with JSON-Schema argument
  validation, timeouts, and output caps.
- Identical behavior from the CLI (`rep …`) and the MCP server (`rep mcp`).
- Skill installer for Claude Code, OpenCode, and Codex (`rep install`).
