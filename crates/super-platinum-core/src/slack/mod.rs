pub mod api;
pub mod client;
pub mod edge;
pub mod events;
pub mod huddle_api;
pub mod models;
pub mod realtime;
pub mod transport;
pub mod xparams;

pub use client::{PreparedRequest, SlackClient, SlackClientConfig, api_host};
pub use transport::Transport;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("file upload canceled")]
    UploadCanceled,
    #[error("missing workspace session for team {0}")]
    MissingWorkspace(models::TeamId),
    #[error("Slack API returned error: {0}")]
    Api(String),
    #[error("Slack rate limited request; retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("Slack HTTP status {status}; retry after {retry_after_secs:?} seconds")]
    HttpStatus {
        status: u16,
        retry_after_secs: Option<u64>,
    },
    #[error("transport error: {0}")]
    Transport(String),
    /// The request never reached Slack: no route, no DNS, a refused connect, or
    /// a socket that died mid-flight. Held apart from `Transport` because the
    /// shell answers it with the rail's connection indicator instead of pasting
    /// a signed Slack URL into a toast the reader cannot act on.
    #[error("network unavailable: {0}")]
    Offline(String),
    #[error("transport not up")]
    TransportNotConfigured,
}

impl Error {
    /// True when the failure is the machine's own link, not anything Slack said.
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }
}
