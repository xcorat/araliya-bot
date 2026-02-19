//! Agents subsystem — receives agent-targeted requests and routes to agents.

use std::collections::{HashMap, HashSet};

use crate::config::AgentsConfig;
use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

/// Basic agents runtime with method-based routing.
///
/// Method grammar:
/// - `agents`                         -> default agent, default action
/// - `agents/{agent_id}`              -> explicit agent, default action
/// - `agents/{agent_id}/{action}`     -> explicit agent + action
pub struct AgentsSubsystem {
    enabled_agents: HashSet<String>,
    default_agent: String,
    channel_map: HashMap<String, String>,
}

impl AgentsSubsystem {
    pub fn new(config: AgentsConfig) -> Self {
        let mut enabled = config.enabled;
        if enabled.is_empty() {
            enabled.push("echo".to_string());
        }

        let default_agent = enabled[0].clone();
        let enabled_agents = enabled.into_iter().collect();

        Self {
            enabled_agents,
            default_agent,
            channel_map: config.channel_map,
        }
    }

    pub fn handle_request(&self, method: &str, payload: BusPayload) -> BusResult {
        let (method_agent_id, _action) = parse_method(method)?;

        match payload {
            BusPayload::CommsMessage { channel_id, content } => {
                let agent_id = self.resolve_agent(method_agent_id.as_deref(), &channel_id)?;
                self.run_agent(&agent_id, channel_id, content)
            }
            _ => Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("unsupported payload for method: {method}"),
            )),
        }
    }

    fn resolve_agent(&self, method_agent_id: Option<&str>, channel_id: &str) -> Result<String, BusError> {
        if let Some(agent_id) = method_agent_id {
            return if self.enabled_agents.contains(agent_id) {
                Ok(agent_id.to_string())
            } else {
                Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("agent not found: {agent_id}"),
                ))
            };
        }

        if let Some(mapped_agent) = self.channel_map.get(channel_id)
            && self.enabled_agents.contains(mapped_agent)
        {
            return Ok(mapped_agent.clone());
        }

        Ok(self.default_agent.clone())
    }

    fn run_agent(&self, agent_id: &str, channel_id: String, content: String) -> BusResult {
        match agent_id {
            "basic_chat" => Ok(BusPayload::CommsMessage {
                channel_id,
                content,
            }),
            "echo" => Ok(BusPayload::CommsMessage {
                channel_id,
                content,
            }),
            _ => Err(BusError::new(
                ERR_METHOD_NOT_FOUND,
                format!("agent not found: {agent_id}"),
            )),
        }
    }
}

fn parse_method(method: &str) -> Result<(Option<String>, String), BusError> {
    let parts: Vec<&str> = method.split('/').collect();

    if parts.is_empty() || parts[0] != "agents" {
        return Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        ));
    }

    match parts.len() {
        1 => Ok((None, "handle".to_string())),
        2 => Ok((Some(parts[1].to_string()), "handle".to_string())),
        3 => Ok((Some(parts[1].to_string()), parts[2].to_string())),
        _ => Err(BusError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_to_default_agent_when_unmapped() {
        let cfg = AgentsConfig {
            enabled: vec!["basic_chat".to_string()],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg);

        let res = agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hello".to_string(),
            },
        );

        match res {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "hello"),
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn routes_by_channel_mapping() {
        let mut channel_map = HashMap::new();
        channel_map.insert("pty0".to_string(), "echo".to_string());

        let cfg = AgentsConfig {
            enabled: vec!["echo".to_string()],
            channel_map,
        };
        let agents = AgentsSubsystem::new(cfg);

        let res = agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "mapped".to_string(),
            },
        );

        match res {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "mapped"),
            _ => panic!("unexpected response"),
        }
    }

    #[test]
    fn explicit_unknown_agent_errors() {
        let cfg = AgentsConfig {
            enabled: vec!["basic_chat".to_string()],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg);

        let res = agents.handle_request(
            "agents/unknown",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "hi".to_string(),
            },
        );

        assert!(res.is_err());
    }

    #[test]
    fn empty_enabled_falls_back_to_echo() {
        let cfg = AgentsConfig {
            enabled: vec![],
            channel_map: HashMap::new(),
        };
        let agents = AgentsSubsystem::new(cfg);

        let res = agents.handle_request(
            "agents",
            BusPayload::CommsMessage {
                channel_id: "pty0".to_string(),
                content: "fallback".to_string(),
            },
        );

        match res {
            Ok(BusPayload::CommsMessage { content, .. }) => assert_eq!(content, "fallback"),
            _ => panic!("unexpected response"),
        }
    }
}
