//! Chat-family agent plugins and their shared [`ChatCore`].
//!
//! ```text
//! ChatCore::basic_complete()        ← shared logic lives here
//!     ↑                    ↑
//! BasicChatPlugin     SessionChatPlugin  (calls core + future extensions)
//! ```

pub mod core;

#[cfg(feature = "plugin-basic-chat")]
pub(crate) mod basic_chat;

#[cfg(feature = "plugin-chat")]
pub(crate) mod session_chat;

// Re-exports so the parent mod can register plugins without reaching into submodules.
#[cfg(feature = "plugin-basic-chat")]
pub(crate) use basic_chat::BasicChatPlugin;

#[cfg(feature = "plugin-chat")]
pub(crate) use session_chat::SessionChatPlugin;
