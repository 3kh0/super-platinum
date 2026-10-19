use super::*;
use super_platinum_core::slack_lists::comments::journal::{Journal, Scope as DiskScope};
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Disk {
    pub account: String,
    pub scope: DiskScope,
    path: Option<std::path::PathBuf>,
}
impl Disk {
    pub fn capture(shell: &ShellState, owner: &Owner, target: &Target) -> Result<Self, Error> {
        if !owner.active(shell) {
            return Err(unavailable());
        }
        let account = shell.core.active_account.clone().ok_or_else(unavailable)?;
        uuid::Uuid::parse_str(&account).map_err(|_| unavailable())?;
        let path = if crate::fixture::is_fixture() {
            Some(fixture::compose::path(shell).ok_or_else(unavailable)?)
        } else {
            None
        };
        let scope = DiskScope::capture(&owner.scope.team, &owner.scope.stable_owner().2, target)
            .map_err(|_| unavailable())?;
        Ok(Self {
            account,
            scope,
            path,
        })
    }
    pub async fn work<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal) -> Result<T, super_platinum_core::error::AppError> + Send + 'static,
    ) -> Result<T, Error> {
        let disk = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut journal = match disk.path {
                Some(path) => Journal::open(path, &disk.account),
                None => Journal::open_default(&disk.account),
            }
            .map_err(|_| unavailable())?;
            f(&mut journal).map_err(|_| Error::Api("list_comment_storage_unavailable".into()))
        })
        .await
        .map_err(|_| unavailable())?
    }
}

#[cfg(test)]
impl Disk {
    pub(in crate::slack_lists::comments) fn test(
        scope: DiskScope,
        path: std::path::PathBuf,
    ) -> Self {
        Self {
            account: "11111111-2222-4333-8444-555555555555".into(),
            scope,
            path: Some(path),
        }
    }
}
