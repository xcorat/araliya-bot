//! Bot identity — ed25519 keypair generation, persistence, and `bot_id` derivation.
//!
//! Layout under `work_dir`:
//! ```text
//! ~/.araliya/
//! └── bot-pkey{8-hex-chars}/
//!     ├── id_ed25519       (32-byte signing key seed, mode 0600)
//!     └── id_ed25519.pub   (32-byte verifying key, mode 0644)
//! ```
//!
//! `bot_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use sha2::{Digest, Sha256};

use crate::{config::Config, error::AppError};

/// Loaded bot identity.
#[derive(Debug, Clone)]
pub struct Identity {
    /// First 8 hex chars of `SHA256(verifying_key)`.
    pub bot_id: String,
    /// Path to the identity directory (`work_dir/bot-pkey{bot_id}/`).
    pub identity_dir: PathBuf,
    verifying_key: [u8; 32],
    signing_key_seed: [u8; 32],
}

impl Identity {
    pub fn verifying_key_bytes(&self) -> &[u8; 32] {
        &self.verifying_key
    }
}

/// Load or create the bot identity under `config.work_dir`.
pub fn setup(config: &Config) -> Result<Identity, AppError> {
    let explicit_identity_dir = config.identity_dir.clone();

    let (signing_seed, verifying_bytes, identity_dir) = if let Some(dir) = explicit_identity_dir {
        if dir.exists() {
            let (seed, vk) = load_keypair(&dir)?;
            (seed, vk, dir)
        } else {
            let (seed, vk) = generate_keypair();
            fs::create_dir_all(&dir)
                .map_err(|e| AppError::Identity(format!("cannot create identity dir: {e}")))?;
            save_keypair(&dir, &seed, &vk)?;
            (seed, vk, dir)
        }
    } else {
        // We need the bot_id to name the directory, but the id comes from the key.
        // Strategy: use a single discovered `bot-pkey*` directory if unambiguous, else generate.
        let dirs = find_existing_identity_dirs(&config.work_dir)?;
        match dirs.len() {
            0 => {
                let (seed, vk) = generate_keypair();
                let bot_id = compute_bot_id(&vk);
                let dir = config.work_dir.join(format!("bot-pkey{}", bot_id));
                fs::create_dir_all(&dir)
                    .map_err(|e| AppError::Identity(format!("cannot create identity dir: {e}")))?;
                save_keypair(&dir, &seed, &vk)?;
                (seed, vk, dir)
            }
            1 => {
                let dir = &dirs[0];
                let (seed, vk) = load_keypair(dir)?;
                (seed, vk, dir.clone())
            }
            _ => {
                return Err(AppError::Identity(format!(
                    "multiple identity directories found in {} ({}); set [supervisor].identity_dir explicitly",
                    config.work_dir.display(),
                    dirs.iter()
                        .map(|d| d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| d.display().to_string()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
    };

    let bot_id = compute_bot_id(&verifying_bytes);

    Ok(Identity {
        bot_id,
        identity_dir,
        verifying_key: verifying_bytes,
        signing_key_seed: signing_seed,
    })
}

// ── internals ────────────────────────────────────────────────────────────────

/// Generate a new ed25519 keypair. Returns `(signing_key_seed, verifying_key_bytes)`.
fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_bytes: [u8; 32] = signing_key.verifying_key().to_bytes();
    let seed: [u8; 32] = signing_key.to_bytes();
    (seed, verifying_bytes)
}

/// Derive `bot_id`: first 8 hex chars of `SHA256(verifying_key_bytes)`.
pub fn compute_bot_id(verifying_key_bytes: &[u8; 32]) -> String {
    let digest = Sha256::digest(verifying_key_bytes);
    hex::encode(digest)[..8].to_string()
}

/// Save keypair to `dir/id_ed25519` (seed, 0600) and `dir/id_ed25519.pub` (vk, 0644).
fn save_keypair(dir: &Path, seed: &[u8; 32], vk: &[u8; 32]) -> Result<(), AppError> {
    let secret_path = dir.join("id_ed25519");
    let pub_path = dir.join("id_ed25519.pub");

    fs::write(&secret_path, seed)
        .map_err(|e| AppError::Identity(format!("cannot write id_ed25519: {e}")))?;
    fs::write(&pub_path, vk)
        .map_err(|e| AppError::Identity(format!("cannot write id_ed25519.pub: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&secret_path, fs::Permissions::from_mode(0o600))
            .map_err(|e| AppError::Identity(format!("cannot set permissions on id_ed25519: {e}")))?;
        fs::set_permissions(&pub_path, fs::Permissions::from_mode(0o644))
            .map_err(|e| AppError::Identity(format!("cannot set permissions on id_ed25519.pub: {e}")))?;
    }

    Ok(())
}

/// Load keypair from `dir/id_ed25519` and `dir/id_ed25519.pub`.
fn load_keypair(dir: &Path) -> Result<([u8; 32], [u8; 32]), AppError> {
    let seed_bytes = fs::read(dir.join("id_ed25519"))
        .map_err(|e| AppError::Identity(format!("cannot read id_ed25519: {e}")))?;
    let vk_bytes = fs::read(dir.join("id_ed25519.pub"))
        .map_err(|e| AppError::Identity(format!("cannot read id_ed25519.pub: {e}")))?;

    let seed: [u8; 32] = seed_bytes
        .try_into()
        .map_err(|_| AppError::Identity("id_ed25519 is not 32 bytes".into()))?;
    let vk: [u8; 32] = vk_bytes
        .try_into()
        .map_err(|_| AppError::Identity("id_ed25519.pub is not 32 bytes".into()))?;

    // Validate: reconstruct verifying key from seed and compare.
    let reconstructed = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    if reconstructed != vk {
        return Err(AppError::Identity(
            "keypair mismatch: verifying key does not match signing key seed".into(),
        ));
    }

    Ok((seed, vk))
}

/// Scan `work_dir` for `bot-pkey*` subdirectories containing `id_ed25519`.
fn find_existing_identity_dirs(work_dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    if !work_dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(work_dir)
        .map_err(|e| AppError::Identity(format!("cannot read work_dir: {e}")))?;
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("bot-pkey") && entry.path().join("id_ed25519").exists() {
            candidates.push(entry.path());
        }
    }
    candidates.sort();
    Ok(candidates)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;

    fn test_config(work_dir: &Path) -> Config {
        Config::test_default(work_dir)
    }

    #[test]
    fn compute_bot_id_is_8_hex_chars() {
        let (_, vk) = generate_keypair();
        let id = compute_bot_id(&vk);
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_produces_unique_keys() {
        let (seed1, _) = generate_keypair();
        let (seed2, _) = generate_keypair();
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let (seed, vk) = generate_keypair();
        save_keypair(dir.path(), &seed, &vk).unwrap();
        let (loaded_seed, loaded_vk) = load_keypair(dir.path()).unwrap();
        assert_eq!(seed, loaded_seed);
        assert_eq!(vk, loaded_vk);
    }

    #[test]
    fn setup_creates_identity_dir_and_files() {
        let tmp = TempDir::new().unwrap();
        let cfg = test_config(tmp.path());
        let identity = setup(&cfg).unwrap();

        assert!(identity.identity_dir.exists());
        assert!(identity.identity_dir.join("id_ed25519").exists());
        assert!(identity.identity_dir.join("id_ed25519.pub").exists());
        assert_eq!(identity.bot_id.len(), 8);
    }

    #[test]
    fn setup_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let cfg = test_config(tmp.path());
        let id1 = setup(&cfg).unwrap();
        let id2 = setup(&cfg).unwrap();
        assert_eq!(id1.bot_id, id2.bot_id);
    }

    #[test]
    fn setup_errors_when_multiple_identity_dirs_exist_without_explicit_config() {
        let tmp = TempDir::new().unwrap();

        let (seed_a, vk_a) = generate_keypair();
        let dir_a = tmp.path().join("bot-pkeyaaaa1111");
        fs::create_dir_all(&dir_a).unwrap();
        save_keypair(&dir_a, &seed_a, &vk_a).unwrap();

        let (seed_b, vk_b) = generate_keypair();
        let dir_b = tmp.path().join("bot-pkeybbbb2222");
        fs::create_dir_all(&dir_b).unwrap();
        save_keypair(&dir_b, &seed_b, &vk_b).unwrap();

        let cfg = test_config(tmp.path());
        let err = setup(&cfg).unwrap_err();
        assert!(err.to_string().contains("multiple identity directories found"));
    }

    #[test]
    fn setup_uses_explicit_identity_dir_when_configured() {
        let tmp = TempDir::new().unwrap();

        let (seed_a, vk_a) = generate_keypair();
        let dir_a = tmp.path().join("bot-pkeyaaaa1111");
        fs::create_dir_all(&dir_a).unwrap();
        save_keypair(&dir_a, &seed_a, &vk_a).unwrap();

        let (seed_b, vk_b) = generate_keypair();
        let dir_b = tmp.path().join("bot-pkeybbbb2222");
        fs::create_dir_all(&dir_b).unwrap();
        save_keypair(&dir_b, &seed_b, &vk_b).unwrap();

        let mut cfg = test_config(tmp.path());
        cfg.identity_dir = Some(dir_b.clone());

        let identity = setup(&cfg).unwrap();
        assert_eq!(identity.identity_dir, dir_b);
    }

    #[cfg(unix)]
    #[test]
    fn secret_key_mode_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let cfg = test_config(tmp.path());
        let identity = setup(&cfg).unwrap();
        let mode = fs::metadata(identity.identity_dir.join("id_ed25519"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
