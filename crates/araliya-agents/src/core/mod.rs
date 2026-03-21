//! Shared utilities and core types for the agents subsystem.
//!
//! ## Contents
//!
//! - [`AgentRuntimeClass`] — v0.6 runtime taxonomy for classifying agents by
//!   execution model.  This is the first-class runtime foundation introduced in
//!   PR1 of the agents v0.6 architecture.
//! - [`agentic`] — shared agentic loop logic used by multi-step agent plugins.
//! - [`prompt`] — prompt assembly helpers shared across agent plugins.
//!
//! Internal helpers live here rather than in the top-level `agents/mod.rs` so
//! that the plugin files stay focused on their own behaviour.

pub mod agentic;
pub mod prompt;

// ── AgentRuntimeClass ─────────────────────────────────────────────────────────

/// Execution model for a registered agent.
///
/// Runtime classes describe *how* an agent processes requests — not what it
/// does or which tools and prompts it uses.  They map directly to the v0.6
/// runtime taxonomy defined in `docs/architecture/subsystems/agents_v0.6.md`.
///
/// ## Current classes (PR1)
///
/// | Variant | Pattern |
/// |---|---|
/// | [`RequestResponse`] | Stateless single-turn exchange — one request, one reply. |
/// | [`Session`] | Persistent multi-turn conversation; session memory is maintained. |
/// | [`Agentic`] | Bounded multi-step orchestration: instruction → tools → response. |
/// | [`Specialized`] | Transitional class for agents whose model does not fit the above. |
///
/// ## Planned classes (deferred to later PRs)
///
/// | Variant | Pattern |
/// |---|---|
/// | [`Workflow`] | Stateful step-graph orchestration with explicit transitions. |
/// | [`Background`] | Event-driven long-running process with its own lifecycle. |
///
/// `Workflow` and `Background` are architectural placeholders in PR1.  They
/// appear as enum variants so they can be referenced in documentation and
/// forward-planning, but they have **no implementation** in this phase.
///
/// ## Built-in agent mappings (PR1)
///
/// | Agent ID | Runtime class |
/// |---|---|
/// | `echo` | `RequestResponse` |
/// | `basic_chat` | `RequestResponse` |
/// | `chat` | `Session` |
/// | `agentic-chat` | `Agentic` |
/// | `docs` | `Agentic` |
/// | `news` | `Specialized` |
/// | `gmail` | `Specialized` |
/// | `runtime_cmd` | `Specialized` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRuntimeClass {
    /// Stateless request-in, response-out.
    ///
    /// No session state is created or required.  The agent receives a single
    /// message and returns a single reply.  The simplest execution model.
    ///
    /// Built-in agents: `echo`, `basic_chat`.
    RequestResponse,

    /// Multi-turn conversation with persistent session memory.
    ///
    /// The agent maintains a transcript across turns.  Session IDs are
    /// preserved and reused across requests from the same user/channel.
    ///
    /// Built-in agents: `chat`.
    Session,

    /// Bounded multi-step orchestration with tool use.
    ///
    /// The agent runs a structured loop: an instruction pass that may emit
    /// tool calls, followed by tool execution, followed by a final response
    /// pass.  Session-aware but distinct from the conversational `Session`
    /// model — tool calls drive the turns rather than user messages.
    ///
    /// Built-in agents: `agentic-chat`, `docs`.
    Agentic,

    /// (Planned — not implemented in PR1.)
    ///
    /// Stateful step-graph orchestration.  A workflow instance progresses
    /// through an explicit set of named steps, with optional checkpointing.
    /// Design is deferred to a later phase.
    Workflow,

    /// (Planned — not implemented in PR1.)
    ///
    /// Event-driven long-running background process.  The agent subscribes to
    /// event sources (bus, cron, external), emits outputs asynchronously, and
    /// has its own start/stop/health lifecycle managed by the supervisor.
    /// Design is deferred to a later phase.
    Background,

    /// Transitional class for agents with specialised execution models.
    ///
    /// Used for built-in agents (`news`, `gmail`, `runtime_cmd`) whose
    /// behaviour does not cleanly map to `RequestResponse`, `Session`, or
    /// `Agentic` in this phase.  This classification may be refined in a
    /// later cleanup PR once the primary runtime classes are stable.
    Specialized,
}

impl AgentRuntimeClass {
    /// Return a stable, lowercase snake_case label suitable for JSON output,
    /// log messages, and future config representation.
    ///
    /// These labels are part of the public API surface — do not change them
    /// without a migration plan.
    pub fn label(self) -> &'static str {
        match self {
            Self::RequestResponse => "request_response",
            Self::Session => "session",
            Self::Agentic => "agentic",
            Self::Workflow => "workflow",
            Self::Background => "background",
            Self::Specialized => "specialized",
        }
    }

    /// Return `true` if this runtime class is fully implemented in the current
    /// phase (v0.6 PR1).  Deferred classes are present as enum variants but
    /// have no execution path.
    pub fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::RequestResponse | Self::Session | Self::Agentic | Self::Specialized
        )
    }
}

impl std::fmt::Display for AgentRuntimeClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::AgentRuntimeClass;

    #[test]
    fn runtime_class_labels_are_stable() {
        assert_eq!(
            AgentRuntimeClass::RequestResponse.label(),
            "request_response"
        );
        assert_eq!(AgentRuntimeClass::Session.label(), "session");
        assert_eq!(AgentRuntimeClass::Agentic.label(), "agentic");
        assert_eq!(AgentRuntimeClass::Workflow.label(), "workflow");
        assert_eq!(AgentRuntimeClass::Background.label(), "background");
        assert_eq!(AgentRuntimeClass::Specialized.label(), "specialized");
    }

    #[test]
    fn runtime_class_display_matches_label() {
        let cases = [
            AgentRuntimeClass::RequestResponse,
            AgentRuntimeClass::Session,
            AgentRuntimeClass::Agentic,
            AgentRuntimeClass::Workflow,
            AgentRuntimeClass::Background,
            AgentRuntimeClass::Specialized,
        ];
        for class in cases {
            assert_eq!(
                format!("{class}"),
                class.label(),
                "Display mismatch for {class:?}"
            );
        }
    }

    #[test]
    fn implemented_classes_are_correct() {
        assert!(AgentRuntimeClass::RequestResponse.is_implemented());
        assert!(AgentRuntimeClass::Session.is_implemented());
        assert!(AgentRuntimeClass::Agentic.is_implemented());
        assert!(AgentRuntimeClass::Specialized.is_implemented());

        // Deferred — must not claim to be implemented.
        assert!(!AgentRuntimeClass::Workflow.is_implemented());
        assert!(!AgentRuntimeClass::Background.is_implemented());
    }

    #[test]
    fn runtime_class_is_copy() {
        let a = AgentRuntimeClass::Session;
        let b = a; // Copy — no move
        assert_eq!(a, b);
    }
}
