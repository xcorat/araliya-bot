# Tools Subsystem

**Status:** Planned — not yet implemented.

---

## Overview

The Tools subsystem owns the tool registry and handles tool execution on behalf of agents. Tools are internal procedures (as opposed to plugins, which are external). Agents request tool execution via the supervisor — they do not call tools directly.

---

## Responsibilities

- Maintain a registry of available tools
- Execute tools in response to `ToolRequest` messages
- Enforce per-session and per-agent tool allowlists
- Apply resource limits (timeout, path sandboxing)

---

## Tool Types

- **Built-in tools** — bundled with the supervisor: `file_ops`, `http_client`, `shell_exec`
- **Plugin tools** — loaded at runtime from a configured directory

---

## Sandboxing

- Filesystem operations are scoped to a relative path; `..` traversal is rejected
- Shell execution has configurable timeout and output size limits
- Future: OS-level process isolation per tool

---

## Message Protocol

- `ToolRequest` — tool name, arguments, calling agent context
- `ToolResponse` — result, error, usage metadata
