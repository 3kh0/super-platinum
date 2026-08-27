use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Slack error: {0}")]
    Slack(#[from] crate::slack::Error),
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
}

pub trait NetworkFailure {
    fn is_offline(&self) -> bool;
}

impl NetworkFailure for crate::slack::Error {
    fn is_offline(&self) -> bool {
        crate::slack::Error::is_offline(self)
    }
}

impl NetworkFailure for AppError {
    fn is_offline(&self) -> bool {
        matches!(self, Self::Slack(error) if error.is_offline())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_dropped_link_reads_as_offline() {
        let offline = AppError::Slack(crate::slack::Error::Offline("client error (Connect)".into()));
        assert!(offline.is_offline());

        let refused = AppError::Slack(crate::slack::Error::Api("channel_not_found".into()));
        assert!(!refused.is_offline());

        let local = AppError::Io(std::io::Error::other("disk"));
        assert!(!local.is_offline());
    }
}
