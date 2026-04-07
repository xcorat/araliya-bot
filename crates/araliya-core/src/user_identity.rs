//! User identity — ed25519 keypair for the human user (separate from bot/agent identities).
//!
//! Layout under `work_dir/users/`:
//! ```text
//! ~/.araliya/users/
//! └── user-{8-hex-chars}/
//!     ├── id_ed25519        (signing key seed, 0600)
//!     ├── id_ed25519.pub    (verifying key, 0644)
//!     └── profile.toml      (user metadata: name, created_at, notes_dir)
//! ```
//!
//! `public_id` is the first 8 hex characters of `SHA256(verifying_key_bytes)`.
//!
//! This module wraps `setup_named_identity()` from the main identity module
//! to provide user-scoped identity creation and loading.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::identity;

// ── Profile TOML ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UserProfile {
    display_name: Option<String>,
    notes_dir: Option<String>,
    created_at: Option<String>,
}

fn read_profile(identity_dir: &std::path::Path) -> UserProfile {
    let path = identity_dir.join("profile.toml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return UserProfile::default();
    };
    toml::from_str(&raw).unwrap_or_default()
}

fn write_profile(identity_dir: &std::path::Path, profile: &UserProfile) -> Result<(), AppError> {
    let path = identity_dir.join("profile.toml");
    let raw = toml::to_string_pretty(profile)
        .map_err(|e| AppError::Identity(format!("cannot serialize profile: {e}")))?;
    std::fs::write(&path, raw)
        .map_err(|e| AppError::Identity(format!("cannot write profile.toml: {e}")))?;
    Ok(())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// User identity wrapper — combines an ed25519 keypair with persisted metadata.
#[derive(Debug, Clone)]
pub struct UserIdentity {
    /// The underlying ed25519 identity (public_id, identity_dir, keys).
    pub identity: identity::Identity,
    /// Display name from profile.toml (may be empty if not set).
    pub display_name: Option<String>,
    /// Path to a markdown notes folder, from profile.toml.
    pub notes_dir: Option<PathBuf>,
}

/// Create or load a user identity from `{work_dir}/users/user-{public_id}/`.
///
/// - If the identity does not exist, generates a new ed25519 keypair and writes `profile.toml`.
/// - If it already exists, reads `profile.toml` for the display name / notes dir;
///   updates the profile if `display_name` or `notes_dir` are provided (non-None).
pub fn create_or_load(
    work_dir: &str,
    display_name: Option<String>,
    notes_dir: Option<PathBuf>,
) -> Result<UserIdentity, AppError> {
    let users_dir = PathBuf::from(work_dir).join("users");
    std::fs::create_dir_all(&users_dir)
        .map_err(|e| AppError::Identity(format!("cannot create users dir: {e}")))?;

    let identity = identity::setup_named_identity(&users_dir, "user")?;
    let identity_dir = &identity.identity_dir;

    // Read existing profile (or default)
    let mut profile = read_profile(identity_dir);

    // Merge in provided values
    let mut dirty = false;
    if let Some(name) = display_name
        && profile.display_name.as_deref() != Some(name.as_str())
    {
        profile.display_name = Some(name);
        dirty = true;
    }
    if let Some(dir) = notes_dir.as_ref() {
        let dir_str = dir.to_string_lossy().into_owned();
        if profile.notes_dir.as_deref() != Some(dir_str.as_str()) {
            profile.notes_dir = Some(dir_str);
            dirty = true;
        }
    }
    if profile.created_at.is_none() {
        profile.created_at = Some(chrono_now());
        dirty = true;
    }

    if dirty {
        write_profile(identity_dir, &profile)?;
    }

    Ok(UserIdentity {
        identity,
        display_name: profile.display_name,
        notes_dir: profile.notes_dir.map(PathBuf::from),
    })
}

fn chrono_now() -> String {
    // Use SystemTime rather than importing chrono — keeps deps minimal.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format as ISO 8601 UTC (YYYY-MM-DDTHH:MM:SSZ) without external crate
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;
    // Simplified date from days since epoch (ignores leap seconds, close enough for a timestamp)
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Gregorian calendar approximation from Julian Day Number
    let mut y = 1970u64;
    let mut d = days;
    loop {
        let leap = is_leap(y);
        let dy = if leap { 366 } else { 365 };
        if d < dy {
            break;
        }
        d -= dy;
        y += 1;
    }
    let leap = is_leap(y);
    let months: &[u64] = if leap {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dm in months {
        if d < dm {
            break;
        }
        d -= dm;
        month += 1;
    }
    (y, month, d + 1)
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_identity_create_and_load() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work_dir = tmp.path().to_string_lossy().into_owned();

        let user1 = create_or_load(&work_dir, Some("Alice".into()), None).expect("create user");
        assert_eq!(user1.display_name, Some("Alice".into()));

        // Reload — should still see Alice, public_id stays same
        let user2 = create_or_load(&work_dir, None, None).expect("load user");
        assert_eq!(user1.identity.public_id, user2.identity.public_id);
        assert_eq!(user2.display_name, Some("Alice".into()));
    }

    #[test]
    fn test_profile_update() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work_dir = tmp.path().to_string_lossy().into_owned();

        create_or_load(&work_dir, Some("Alice".into()), None).expect("first");
        let u = create_or_load(&work_dir, Some("Alicia".into()), None).expect("update");
        assert_eq!(u.display_name, Some("Alicia".into()));
    }

    #[test]
    fn test_profile_toml_written() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work_dir = tmp.path().to_string_lossy().into_owned();

        let u = create_or_load(&work_dir, Some("Bob".into()), None).expect("create");
        let profile_path = u.identity.identity_dir.join("profile.toml");
        assert!(profile_path.exists(), "profile.toml should be written");
        let raw = std::fs::read_to_string(&profile_path).expect("read");
        assert!(raw.contains("Bob"));
        assert!(raw.contains("created_at"));
    }
}
