# Bot Identity

## Overview

Each Araliya instance has a persistent **ed25519 keypair**. The keypair is generated on first run and then loaded on every subsequent run. It is the basis for:

- A stable `bot_id` that identifies this instance
- (Future) signing outbound events and messages
- (Future) authenticating to external services

## bot_id

`bot_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.

```
verifying_key_bytes (32 bytes)
  → SHA256 → hex string (64 chars)
  → first 8 chars = bot_id
```

Example: `5d16993c`

The `bot_id` names the identity directory:

```
~/.araliya/bot-pkey5d16993c/
```

## File Layout

```
{work_dir}/
└── bot-pkey{bot_id}/
    ├── id_ed25519        32-byte signing key seed (raw bytes, mode 0600)
    └── id_ed25519.pub    32-byte verifying key (raw bytes, mode 0644)
```

- `id_ed25519` — the secret key seed. Must be kept private. Mode `0600` (owner read/write only).
- `id_ed25519.pub` — the public verifying key. Safe to share. Mode `0644`.

## Lifecycle

```
identity::setup(&config)
  ├─ scan work_dir for bot-pkey*/ directory containing id_ed25519
  ├─ if found:
  │   ├─ load id_ed25519 (32 bytes)
  │   ├─ load id_ed25519.pub (32 bytes)
  │   ├─ reconstruct verifying key from seed
  │   ├─ verify reconstructed vk == stored pub (integrity check)
  │   └─ return Identity
  └─ if not found:
      ├─ generate new ed25519 keypair (OsRng)
      ├─ compute bot_id from verifying key
      ├─ create {work_dir}/bot-pkey{bot_id}/
      ├─ save id_ed25519 (mode 0600)
      ├─ save id_ed25519.pub (mode 0644)
      └─ return Identity
```

## Security Notes

- The secret key seed (`id_ed25519`) file mode is enforced to `0600` on Unix at creation time
- The key is never logged or printed
- Backup `id_ed25519` to retain bot identity across machine changes; losing it generates a new identity with a different `bot_id`

## Identity Struct

```rust
pub struct Identity {
    pub bot_id: String,       // "5d16993c"
    pub identity_dir: PathBuf // ~/.araliya/bot-pkey5d16993c/
    // private fields: verifying_key, signing_key_seed
}
```
