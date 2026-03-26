//! User identity — ed25519 keypair for the human user (separate from bot/agent identities).
//!
//! Layout under `work_dir/users/`:
//! ```text
//! ~/.araliya/users/
//! └── user-{8-hex-chars}/
//!     ├── id_ed25519        (signing key seed, 0600)
//!     ├── id_ed25519.pub    (verifying key, 0644)
//!     └── profile.toml      (user metadata: name, created_at, password_hash, ssh_pubkey_ref)
//! ```
//!
//! `public_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.
//!
//! This module wraps `setup_named_identity()` from the main identity module
//! to provide user-scoped identity creation and loading.

use std::path::PathBuf;

use crate::identity;
use crate::error::AppError;

/// User identity wrapper — combines an ed25519 keypair with optional metadata.
#[derive(Debug, Clone)]
pub struct UserIdentity {
    /// The underlying ed25519 identity (public_id, identity_dir, keys).
    pub identity: identity::Identity,
    /// Optional display name for the user.
    pub display_name: Option<String>,
    /// Optional path to a markdown notes folder.
    pub notes_dir: Option<PathBuf>,
}

/// Create or load a user identity from `{work_dir}/users/user-{public_id}/`.
///
/// If the identity does not exist, generates a new ed25519 keypair.
/// Returns `UserIdentity` with optional metadata fields.
pub fn create_or_load(
    work_dir: &str,
    display_name: Option<String>,
    notes_dir: Option<PathBuf>,
) -> Result<UserIdentity, AppError> {
    let users_dir = PathBuf::from(work_dir).join("users");
    std::fs::create_dir_all(&users_dir)
        .map_err(|e| AppError::Identity(format!("cannot create users dir: {e}")))?;

    let identity = identity::setup_named_identity(&users_dir, "user")?;

    Ok(UserIdentity {
        identity,
        display_name,
        notes_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_identity_create_and_load() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work_dir = tmp.path().to_string_lossy().into_owned();

        let user1 = create_or_load(&work_dir, Some("Alice".into()), None)
            .expect("create first user");
        let user1_id = user1.identity.public_id.clone();

        let user2 = create_or_load(&work_dir, Some("Bob".into()), None)
            .expect("create second user (should fail or reuse)");

        // Both should load the same identity (since we only create one per prefix)
        assert_eq!(user1_id, user2.identity.public_id);
        assert_eq!(user1.display_name, Some("Alice".into()));
        assert_eq!(user2.display_name, Some("Bob".into()));
    }
}
