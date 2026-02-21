# Agent and Subagent Identity Implementation

## Overview
The identity system has been generalized to support not only the main bot (supervisor) but also individual agents and their subagents. This allows agents to have their own cryptographic identities (`ed25519` keypairs) and isolated storage boundaries.

## Key Changes
1. **`Identity` Struct Generalization**:
   - `bot_id` was renamed to `public_id` to reflect its generic use across the bot, agents, and subagents.
   - `compute_bot_id` was renamed to `compute_public_id`.

2. **Named Identities**:
   - Added `identity::setup_named_identity(base_dir: &Path, prefix: &str) -> Result<Identity, AppError>`.
   - This function scans for or creates a directory starting with `{prefix}-` under `base_dir`. If it doesn't exist, it generates a new keypair, computes the `public_id`, and saves it in `{base_dir}/{prefix}-{public_id}`.

3. **Agent Identities**:
   - During `AgentsSubsystem` initialization, it iterates over all registered agents and provisions an identity for each using `setup_named_identity`.
   - These identities are stored in `memory/agent/{agent_name}-{public_id}/`.
   - The identities are kept in `AgentsState::agent_identities` (`HashMap<String, Identity>`), allowing agents to access their own keys during request handling.

4. **Subagent Identities**:
   - Subagents are ephemeral or task-specific workers spawned by an agent.
   - They get their own cryptographic identity and folder nested under their parent agent's memory directory: `memory/agent/{agent_name}-{public_id}/subagents/{subagent_name}-{public_id}/`.
   - Agents can provision subagents using `AgentsState::get_or_create_subagent(agent_id: &str, subagent_name: &str) -> Result<Identity, AppError>`.

## Directory Layout
```text
~/.araliya/
└── bot-pkey{bot_public_id}/
    ├── id_ed25519
    ├── id_ed25519.pub
    └── memory/
        └── agent/
            └── {agent_name}-{agent_public_id}/
                ├── id_ed25519
                ├── id_ed25519.pub
                └── subagents/
                    └── {subagent_name}-{subagent_public_id}/
                        ├── id_ed25519
                        └── id_ed25519.pub
```

## Future Work
- Update architecture documentation to reflect the new identity hierarchy.
- Implement the "pipe" abstraction for session storage if required.
- Add more comprehensive tests for subagent lifecycle and memory isolation.
