//! The vocabulary the rest of the crate speaks: identities, versions, and the
//! addon shapes that sources produce and the installer consumes.

pub mod addon;
pub mod description;
pub mod expansion;
pub mod folder;
pub mod history;
pub mod sort;
pub mod source_id;
pub mod version;

pub use addon::{AddonDetail, AddonId, AddonKey, AddonSummary, Download, Screenshot};
pub use description::{Markup, SafeHtml, keep_link};
pub use expansion::Expansion;
pub use folder::{AddonFolder, FolderNameError};
pub use history::{
    Channel, Dependency, FileHistory, FileKey, FileListing, PublishedFile, RelatedAddon, Relation,
    mentioned_addons,
};
pub use sort::{Sort, SortDirection, SortField};
pub use source_id::{CatalogSource, Listing, LiveSource, SourceId};
pub use version::AddonVersion;
