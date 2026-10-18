//! Passive request-local evidence of entering the HTTP send future.
//! Not proof of Slack receipt, success, permission, or canonical publication.
use super::*;
use crate::{
    config::WorkspaceSession,
    slack::{SlackClient, api, models::ChannelId},
};
use std::{collections::BTreeMap, future::Future, sync::atomic::AtomicBool};

#[derive(Clone)]
pub struct ChannelReadReceipt(Arc<AtomicBool>);
impl ChannelReadReceipt {
    pub fn entered_send(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    // First poll of a built HTTP request, after local link and HTTP-client gates.
    // Kept outside caller timeout futures so cancellation cannot erase evidence.
    pub(super) async fn observe<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        std::future::poll_fn(|cx| {
            self.0.store(true, Ordering::Release);
            future.as_mut().poll(cx)
        })
        .await
    }
}
enum Kind {
    Directory,
    Generic,
    Setup,
    Split,
}
/// Non-Clone, consumed by execute_channel_read. Its passive receipt can be
/// cloned, but cannot be supplied to a second or unrelated request.
pub struct ChannelReadAttempt {
    request: PreparedRequest,
    kind: Kind,
    receipt: ChannelReadReceipt,
}
impl ChannelReadAttempt {
    pub fn initial_init(client: &SlackClient, workspace: &WorkspaceSession) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::initial_init(
                client, workspace,
            ),
            Kind::Split,
        )
    }
    pub fn initial_channels(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        version_all_channels: bool,
        warm: bool,
        min_channel_updated: Option<u64>,
    ) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::initial_channels(
                client,
                workspace,
                version_all_channels,
                warm,
                min_channel_updated,
            ),
            Kind::Split,
        )
    }
    pub fn initial_extras(client: &SlackClient, workspace: &WorkspaceSession) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::initial_extras(
                client, workspace,
            ),
            Kind::Split,
        )
    }
    pub fn directory(request: PreparedRequest) -> Result<Self, Error> {
        if !super::directory_channel_info::allowed(&request) {
            return Err(Error::Api("invalid_directory_channel_info_request".into()));
        }
        Ok(Self::new(request, Kind::Directory))
    }
    pub fn generic(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        updated: &BTreeMap<ChannelId, u64>,
    ) -> Self {
        Self::new(
            api::conversations_generic_info(client, workspace, updated),
            Kind::Generic,
        )
    }
    /// Exact existing setup factories. Their receipts can retain issued errors
    /// after retirement, but do not attest any channel-ID freshness attempt.
    pub fn assignments(client: &SlackClient, workspace: &WorkspaceSession) -> Self {
        Self::new(api::list_experiments(client, workspace), Kind::Setup)
    }
    pub fn discovery(client: &SlackClient, workspace: &WorkspaceSession) -> Result<Self, Error> {
        Ok(Self::new(
            crate::block_actions::directory_endpoint::DirectoryEndpoint::discovery(
                client, workspace,
            )?,
            Kind::Setup,
        ))
    }
    /// Exact split resync factories. These receipts account for original
    /// branch transport attempts only; they grant no channel-ID freshness.
    pub fn resync_init(client: &SlackClient, workspace: &WorkspaceSession) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::init(client, workspace),
            Kind::Split,
        )
    }
    pub fn resync_channels(
        client: &SlackClient,
        workspace: &WorkspaceSession,
        version_all_channels: bool,
    ) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::channels(
                client,
                workspace,
                version_all_channels,
            ),
            Kind::Split,
        )
    }
    pub fn resync_extras(client: &SlackClient, workspace: &WorkspaceSession) -> Self {
        Self::new(
            crate::state::directory_channels::freshness::refresh::request::extras(
                client, workspace,
            ),
            Kind::Split,
        )
    }
    fn new(request: PreparedRequest, kind: Kind) -> Self {
        Self {
            request,
            kind,
            receipt: ChannelReadReceipt(Arc::new(AtomicBool::new(false))),
        }
    }
    pub fn receipt(&self) -> ChannelReadReceipt {
        self.receipt.clone()
    }
}
pub enum ChannelReadResponse {
    Directory(DirectoryChannelInfoResponse),
    Generic(Value),
    Setup(Value),
    Split { raw: Value, bytes: String },
}
impl Transport {
    /// Existing link-hold, safe-call cancellation, retry and error policy. This
    /// is a read-only observation wrapper, not a new request/admission policy.
    pub async fn execute_channel_read(
        &self,
        read: ChannelReadAttempt,
    ) -> Result<ChannelReadResponse, Error> {
        self.execute_channel_read_current(read, || true).await
    }
    /// Additional original-owner admission, after a local link hold and before
    /// each built request's first HTTP poll. No suspension occurs between that
    /// final check and execute_channel_read_once entering its send future.
    /// The callback can only refuse the already-consumed closed request.
    /// An issued error survives retirement before retry, including Retry-After.
    pub async fn execute_channel_read_current(
        &self,
        read: ChannelReadAttempt,
        current: impl Fn() -> bool,
    ) -> Result<ChannelReadResponse, Error> {
        let mut attempt = 0;
        let mut last_failure = None;
        loop {
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("channel read owner retired".into())));
            }
            let link = self.await_link().await;
            if !current() {
                return Err(last_failure
                    .unwrap_or_else(|| Error::Offline("channel read owner retired".into())));
            }
            link?;
            let attempted = if read.request.retry_safe() {
                tokio::select! {
                    biased;
                    result=self.execute_channel_read_once(&read)=>result,
                    ()=self.link_dropped()=>Err(Error::Offline("link dropped mid-call".into())),
                }
            } else {
                self.execute_channel_read_once(&read).await
            };
            match attempted {
                Ok(value) => return Ok(value),
                Err(e) if read.request.retry_safe() && retryable_error(&e) && attempt < 2 => {
                    attempt += 1;
                    last_failure = Some(e.clone());
                    tokio::time::sleep(retry_delay(&e, attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
    async fn execute_channel_read_once(
        &self,
        read: &ChannelReadAttempt,
    ) -> Result<ChannelReadResponse, Error> {
        let mut keys = None;
        let mut bytes = None;
        let raw = self
            .execute_once_using_observed_decode_mode(
                &read.request,
                false,
                self.http()?,
                false,
                if matches!(read.kind, Kind::Directory) {
                    Some(&mut keys)
                } else {
                    None
                },
                Some(&read.receipt),
                if matches!(read.kind, Kind::Split) {
                    Some(&mut bytes)
                } else {
                    None
                },
            )
            .await?;
        Ok(match read.kind {
            Kind::Directory => {
                ChannelReadResponse::Directory(DirectoryChannelInfoResponse { raw, keys })
            }
            Kind::Generic => ChannelReadResponse::Generic(raw),
            Kind::Setup => ChannelReadResponse::Setup(raw),
            Kind::Split => ChannelReadResponse::Split {
                raw,
                bytes: bytes
                    .ok_or_else(|| Error::Transport("missing split response bytes".into()))?,
            },
        })
    }
}
#[cfg(test)]
mod tests;
