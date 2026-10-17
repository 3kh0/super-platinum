//! Renderer-neutral Canvas creation custody and account-scoped local storage.
//! Blocking SQLite methods belong on a retained off-thread desktop worker.
//! The validated default opener uses the existing config namespace, never the
//! secret backend. This module contains no network executor.
mod model;
mod opener;
mod retirement_model;
mod store;
pub use model::*;
pub use retirement_model::*;
pub use store::{LocalVersionChoice, RetirementReview, Store};

#[derive(Debug, thiserror::Error)]
pub enum Fault {
    #[error("Canvas creation storage is invalid")]
    Invalid,
    #[error("Canvas creation changed; review the retained version")]
    Stale,
    #[error("Canvas creation has an unresolved delivery")]
    Blocked,
    #[error("Canvas creation storage is full; input must remain retained")]
    Capacity,
    #[error("This Canvas operation was explicitly retired")]
    Retired,
    #[error("Canvas creation is rate limited")]
    Cooldown,
    #[error("Canvas creation storage I/O failed")]
    Io(#[from] std::io::Error),
    #[error("Canvas creation database failed")]
    Sql(#[from] rusqlite::Error),
}
pub type Result<T> = std::result::Result<T, Fault>;
