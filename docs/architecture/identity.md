# Identity

## Overview

Each Araliya instance, as well as its individual agents and subagents, has a persistent **ed25519 keypair**. The keypair is generated on first run and then loaded on every subsequent run. It is the basis for:

- A stable `public_id` that identifies the entity (bot, agent, or subagent)
- (Future) signing outbound events and messages
- (Future) authenticating to external services

## public_id

`public_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.

```
verifying_key_bytes (32 bytes)
  → SHA256 → hex string (64 chars)
  → first 8 chars = public_id
```

Example: `5d16993c`

The `public_id` names the identity directory for the bot:

```
~/.araliya/bot-pkey5d16993c/
```

## File Layout

The identity system is hierarchical. The main bot identity sits at the root, and agent/subagent identities are nested within the bot's memory directory. Any prompts must be saved as text to minimize prompt injection.

```
{work_dir}/
└── bot-pkey{bot_public_id}/
    ├── id_ed25519        32-byte signing key seed (raw bytes, mode 0600)
    ├── id_ed25519.pub    32-byte verifying key (raw bytes, mode 0644)
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

- `id_ed25519` — the secret key seed. Must be kept private. Mode `0600` (owner read/write only).
- `id_ed25519.pub` — the public verifying key. Safe to share. Mode `0644`.

## Lifecycle

### Bot Identity
```
identity::setup(&config)
  ├─ scan work_dir for bot-pkey*/ directory containing id_ed25519
  ├─ if found:
  │   ├─ load id_ed25519 & id_ed25519.pub
  │   ├─ reconstruct verifying key from seed
  │   ├─ verify reconstructed vk == stored pub (integrity check)
  │   └─ return Identity
  └─ if not found:
      ├─ generate new ed25519 keypair (OsRng)
      ├─ compute public_id from verifying key
      ├─ create {work_dir}/bot-pkey{public_id}/
      ├─ save id_ed25519 (mode 0600) & id_ed25519.pub (mode 0644)
      └─ return Identity
```

### Agent & Subagent Identities
Agents and subagents use `identity::setup_named_identity(base_dir, prefix)`. This function scans the `base_dir` for a directory starting with `{prefix}-`. If found, it loads the keys. If not, it generates a new keypair, computes the `public_id`, and creates the directory `{prefix}-{public_id}`.

## Security Notes

- The secret key seed (`id_ed25519`) file mode is enforced to `0600` on Unix at creation time
- The key is never logged or printed
- Backup `id_ed25519` to retain identity across machine changes; losing it generates a new identity with a different `public_id`

## Identity Struct

```rust
pub struct Identity {
    pub public_id: String,    // "5d16993c"
    pub identity_dir: PathBuf // ~/.araliya/bot-pkey5d16993c/
    // private fields: verifying_key, signing_key_seed
}
```
