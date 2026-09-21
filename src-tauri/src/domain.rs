//! The vocabulary the rest of the crate speaks: identities, versions, and the
//! addon shapes that sources produce and the installer consumes.

pub mod addon;
pub mod expansion;
pub mod folder;
pub mod source_id;
pub mod version;

pub use addon::{AddonDetail, AddonId, AddonKey, AddonSummary, Download, Screenshot};
pub use expansion::Expansion;
pub use folder::{AddonFolder, FolderNameError};
pub use source_id::SourceId;
pub use version::AddonVersion;
