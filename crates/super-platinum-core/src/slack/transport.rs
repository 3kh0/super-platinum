use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::watch;

use serde_json::Value;
use wreq::header::HeaderMap;
use wreq_util::Emulation;

use super::Error;
use super::client::{PreparedRequest, RequestBody, redact_secrets};

mod avatar;
mod canvas_collab;
mod canvas_edit;
mod draft_mutation;
mod file_pages;
mod file_share_read;
mod file_sort_preference;
mod people_pages;
mod people_sort_preference;
pub use file_share_read::FileShareReadGate;
pub use file_sort_preference::FileSortPreferenceFailure;
pub use people_sort_preference::PeopleSortPreferenceFailure;
mod emoji_preferences;
mod original_download;
mod protected_draft;
pub use canvas_edit::CanvasEditFailure;
pub use protected_draft::{MAX_PROTECTED_DRAFT_RESPONSE_BYTES, ProtectedDraftFailure};
mod global_notifications;
mod list_column_delete;
mod list_column_precision;
mod list_column_rename;
mod list_comments;
pub use list_column_delete::ListColumnDeleteFailure;
pub use list_column_precision::ListColumnPrecisionFailure;
pub use list_comments::{
    ListCommentDeleteFailure, ListCommentReactionFailure, ListCommentReplyFailure,
    ListCommentUpdateFailure,
};
mod list_assignee;
pub use list_assignee::ListAssigneeFailure;
mod list_todo;
pub use list_todo::ListReminderFailure;
mod list_subitem;
pub use list_subitem::ListSubitemFailure;
mod list_board_lane_order;
mod list_board_record_move;
mod list_column_create;
mod list_field_visibility;
mod list_view_filter;
mod list_view_grouping;
pub use list_column_create::ListColumnCreateFailure;
mod list_view_sort;
pub use list_view_filter::ListViewFilterFailure;
pub use list_view_grouping::ListViewGroupingFailure;
pub use list_view_sort::ListViewSortFailure;
mod list_view_create;
mod list_view_rename;
mod list_view_reorder;
pub use list_field_visibility::ListFieldVisibilityFailure;
pub use list_view_create::ListViewCreateFailure;
pub use list_view_rename::ListViewRenameFailure;
pub use list_view_reorder::ListViewReorderFailure;
mod list_column_rename_read;
pub use list_column_rename::ListColumnRenameFailure;
pub use list_column_rename_read::ListColumnRenameReadFailure;
mod activity_member_search;
mod channel_read_dispatch;
mod directory_channel_info;
mod directory_member_info;
mod directory_response;
mod directory_usergroup_info;
mod file_info;
mod list_membership;
mod list_record_delete;
mod mention_search;
mod notification_preferences;
mod notification_schedule;
mod offline_features;
mod own_profile;
mod own_profile_photo;
mod own_profile_recording;
mod presentation_notifications;
mod profile_channel_search;
mod profile_file_search;
mod profile_group_search;
mod profile_long_text_member_search;
mod profile_member_search;
mod profile_sections;
mod read_preferences;
mod read_shortcuts;
mod slash_command;
mod upload_readback;
mod workflow_management;
mod workflow_settings;
mod workflow_share_search;
pub use crate::slack::api::custom::texty::file_title_source::OrderedFileInfo;
pub use crate::slack::api::custom::texty::files::OrderedResponse as ProfileFileSearchResponse;
pub use channel_read_dispatch::{ChannelReadAttempt, ChannelReadReceipt, ChannelReadResponse};
pub use directory_response::DirectoryResponse as WorkflowSettingsResponse;
pub use directory_response::DirectoryResponse as WorkflowShareSearchResponse;
pub use directory_response::DirectoryResponse as ActivityMemberResponse;
pub use directory_response::DirectoryResponse as AutocompleteOfflineResponse;
pub use directory_response::DirectoryResponse as DirectoryMemberInfoResponse;
pub use directory_response::DirectoryResponse as DirectoryUsergroupInfoResponse;
pub use directory_response::DirectoryResponse as DirectoryChannelInfoResponse;
pub use directory_response::DirectoryResponse as ListMembershipResponse;
pub use directory_response::DirectoryResponse as ProfileChannelSearchResponse;
pub use directory_response::DirectoryResponse as ProfileMemberSearchResponse;
pub use directory_response::DirectoryResponse as ProfileGroupSearchResponse;
pub use list_record_delete::ListDeleteFailure;
pub(super) use notification_preferences::notification_preferences_success;
pub use offline_features::AutocompletePrioritiesResponse;

mod admitted;
#[cfg(test)]
mod execute_current_tests;
#[cfg(test)]
mod file_viewing_tests;
#[cfg(test)]
mod list_cell_update_tests;
#[cfg(test)]
mod list_read_state_tests;
#[cfg(test)]
mod list_record_create_tests;

/// What the last Slack API call said about the link to Slack.
///
/// This is the shell's recovery signal as much as its failure signal: a request
/// that lands clears the offline mark without anything having to poll for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Health {
    /// Nothing has been attempted yet.
    #[default]
    Unknown,
    /// Slack answered — even with an error, which still means the link is up.
    Online,
    /// The link failed; an in-flight write may already have reached Slack.
    Offline,
}

#[derive(Clone)]
pub struct Transport {
    http: Option<wreq::Client>,
    /// Strict single-attempt mutations also disable HTTP-library protocol retries.
    /// Lazily pooled so ordinary boot/read paths do not construct another client.
    canvas_http: Arc<std::sync::OnceLock<wreq::Client>>,
    d_cookie: String,
    /// Shared with every clone: the shell holds one and the workers hold others,
    /// and all of them have to agree about whether the network is up. A watch
    /// rather than a flag, because held requests wait on it changing.
    health: Arc<watch::Sender<Health>>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport").finish_non_exhaustive()
    }
}

/// Upper bound on a single media fetch (avatars, emoji, unfurl images, files).
const MEDIA_FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// Large enough for a requested original, but never buffer an unbounded CDN response.
const MAX_MEDIA_BYTES: usize = 64 * 1024 * 1024;

/// How long a call waits for a downed link to come back before giving up.
///
/// The shell confirms recovery by reaching the workspace host, so this is a
/// backstop for the case where that confirmation never arrives — a wait with no
/// end is the hang this whole path exists to remove.
const OFFLINE_HOLD: Duration = Duration::from_secs(60);

/// Upper bound on a single Slack API call.
///
/// Without one, a link that disappears mid-flight leaves the request hanging
/// forever — the pane keeps saying "Loading replies…" and nothing ever reports
/// the failure that would put the connection indicator on the rail. Generous
/// enough for a cold `client.userBoot`, which is the largest of these by far.
const API_TIMEOUT: Duration = Duration::from_secs(30);

impl Transport {
    pub fn new(d_cookie: impl Into<String>) -> Result<Self, Error> {
        let http = wreq::Client::builder()
            .emulation(Emulation::Chrome140)
            // Slack avatar and file URLs commonly redirect to a backing CDN or
            // object store. wreq currently defaults to no redirects; its
            // redirect layer removes sensitive headers on cross-host hops.
            .redirect(wreq::redirect::Policy::limited(10))
            .build()
            .map_err(|e| Error::Transport(format!("client build: {e}")))?;
        Ok(Self {
            http: Some(http),
            canvas_http: Arc::new(std::sync::OnceLock::new()),
            d_cookie: d_cookie.into(),
            health: Arc::new(watch::Sender::new(Health::Unknown)),
        })
    }

    /// What the last Slack API call said about the link.
    pub fn health(&self) -> Health {
        *self.health.borrow()
    }

    /// A view of the link's health for anything that has to react to it rather
    /// than ask — the realtime supervisor, which must not sit on a dead socket
    /// or dial into a link that is known to be gone.
    pub fn health_watch(&self) -> watch::Receiver<Health> {
        self.health.subscribe()
    }

    /// Records a verdict about the link. Media fetches deliberately do not call
    /// this: a third-party image host that is down says nothing about Slack.
    pub fn set_health(&self, health: Health) {
        self.health.send_replace(health);
    }

    /// Resolves when the shell decides the link has gone.
    async fn link_dropped(&self) {
        let mut health = self.health.subscribe();
        let _ = health.wait_for(|health| *health == Health::Offline).await;
    }

    /// Parks a call while the link is known to be down.
    ///
    /// Firing into a dead link is how the shell used to end up with panes stuck
    /// on "Loading…" behind requests that could never land, so a call waits for
    /// the link to be confirmed back instead. Bounded: past the hold it fails
    /// like any other offline call, and the caller's own error path runs.
    async fn await_link(&self) -> Result<(), Error> {
        self.http()?;
        if self.health() != Health::Offline {
            return Ok(());
        }
        let mut health = self.health.subscribe();
        let back = health.wait_for(|health| *health != Health::Offline);
        match tokio::time::timeout(OFFLINE_HOLD, back).await {
            Ok(Ok(_)) => Ok(()),
            _ => Err(Error::Offline(format!(
                "held {}s waiting for the network",
                OFFLINE_HOLD.as_secs()
            ))),
        }
    }

    pub fn http(&self) -> Result<&wreq::Client, Error> {
        self.http
            .as_ref()
            .ok_or_else(|| Error::Offline("disconnected fixture transport".into()))
    }

    pub fn d_cookie(&self) -> &str {
        &self.d_cookie
    }

    fn cookie(&self) -> String {
        format!("d={}", self.d_cookie)
    }

    pub async fn execute(&self, req: PreparedRequest) -> Result<Value, Error> {
        self.execute_current(req, || true).await
    }

    /// Ordinary execution with an original-owner fence at asynchronous dispatch
    /// boundaries. Retry classification, delays and link holds match `execute`.
    /// Retirement does not undo an attempt already sent, but refuses its result
    /// and prevents subsequent attempts. No polling task or timer is added.
    pub async fn execute_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        self.execute_admitted(req, current, || true, |_| {})
            .await?
            .ok_or_else(|| Error::Api("request_superseded".into()))
    }

    /// Preserve structured app feedback for the narrow Block Kit protocol.
    /// Unlike generic API errors, these payloads contain form-field feedback.
    /// Typed endpoint decoders must interpret `ok`; callers must not log values.
    /// Every call is a single attempt, including views.get, so action writes
    /// cannot become retryable through a future generic endpoint policy change.
    /// Writes never wait for recovery: the caller's validated owner and source
    /// may change while offline. The internal unsent marker distinguishes that
    /// preflight rejection from an ambiguous network failure after sending.
    pub async fn execute_block_interaction(&self, req: PreparedRequest) -> Result<Value, Error> {
        if !is_block_interaction_request(&req) {
            return Err(Error::Api("unsupported_block_interaction_request".into()));
        }
        let read_only = url::Url::parse(&req.url).is_ok_and(|url| url.path() == "/api/views.get");
        if read_only {
            self.await_link().await?;
        } else if self.health() == Health::Offline {
            return Err(Error::Api("block_interaction_not_sent_offline".into()));
        }
        self.execute_once_with_response(&req, true).await
    }

    /// File-view presence belongs to the caller's current visible visit. Never
    /// hold it for recovery or retry after sending: that visit may have closed.
    /// Known-offline rejection happens before dispatch; errors after
    /// execute_once starts retain the ordinary transport semantics.
    pub async fn execute_file_viewing(&self, req: PreparedRequest) -> Result<Value, Error> {
        if req.method != "POST"
            || !url::Url::parse(&req.url).is_ok_and(|url| {
                matches!(
                    url.path(),
                    "/api/files.open" | "/api/files.ping" | "/api/files.close"
                )
            })
        {
            return Err(Error::Api("unsupported_file_viewing_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "file viewing request not sent while offline".into(),
            ));
        }
        self.execute_once(&req).await
    }

    /// List-open/read tracking changes server state and belongs to one visible
    /// visit. The caller revalidates that visit immediately before this call.
    /// Never hold for reconnection or replay a request after an uncertain reply.
    pub async fn execute_list_read_state(&self, req: PreparedRequest) -> Result<Value, Error> {
        if req.method != "POST"
            || !url::Url::parse(&req.url).is_ok_and(|url| {
                matches!(
                    url.path(),
                    "/api/lists.open"
                        | "/api/lists.records.open"
                        | "/api/files.markRead"
                        | "/api/lists.getDefaultView"
                )
            })
        {
            return Err(Error::Api("unsupported_list_read_state_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "List read-state request not sent while offline".into(),
            ));
        }
        self.execute_once(&req).await
    }

    /// A List cell write is authorized against a fresh caller-owned snapshot.
    /// Never hold that authorization across recovery or replay an uncertain write.
    pub async fn execute_list_cell_update(&self, req: PreparedRequest) -> Result<Value, Error> {
        if req.method != "POST"
            || !url::Url::parse(&req.url).is_ok_and(|url| url.path() == "/api/lists.cells.update")
        {
            return Err(Error::Api("unsupported_list_cell_update_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "List cell update not sent while offline".into(),
            ));
        }
        self.execute_once(&req).await
    }

    /// A new List record has a server-generated identity and no replay key.
    /// Send once against the caller's fresh authorization; never queue offline.
    pub async fn execute_list_record_create(&self, req: PreparedRequest) -> Result<Value, Error> {
        if req.method != "POST"
            || !url::Url::parse(&req.url).is_ok_and(|url| url.path() == "/api/lists.records.create")
        {
            return Err(Error::Api("unsupported_list_record_create_request".into()));
        }
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "List item creation not sent while offline".into(),
            ));
        }
        // Once dispatch starts, a timeout/reset can mean Slack accepted the
        // new record but its response was lost. The generic transport uses
        // Offline for those failures too; it is not proof of non-delivery.
        // Keep its health update, but preserve an uncertain creation outcome.
        self.execute_once(&req).await.map_err(|error| match error {
            Error::Offline(_) => {
                Error::Transport("List item creation response was lost after dispatch".into())
            }
            error => error,
        })
    }

    pub async fn upload_bytes(
        &self,
        url: &str,
        bytes: Vec<u8>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), Error> {
        let http = self.http()?;
        let length = bytes.len();
        let chunks = bytes
            .chunks(64 * 1024)
            .map(|chunk| chunk.to_vec())
            .collect::<Vec<_>>();
        let stream = futures::stream::iter(chunks.into_iter().map(move |chunk| {
            progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            Ok::<_, std::convert::Infallible>(chunk)
        }));
        let response = http
            .post(url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", length)
            .body(wreq::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(send_error)?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::HttpStatus {
                status: response.status().as_u16(),
                retry_after_secs: retry_after_secs(response.headers()),
            })
        }
    }

    async fn execute_once(&self, req: &PreparedRequest) -> Result<Value, Error> {
        self.execute_once_with_response(req, false).await
    }

    async fn execute_once_with_response(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
    ) -> Result<Value, Error> {
        self.execute_once_using(req, structured_block_response, self.http()?)
            .await
    }

    async fn execute_once_using(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
        http: &wreq::Client,
    ) -> Result<Value, Error> {
        self.execute_once_using_read_mode(req, structured_block_response, http, false)
            .await
    }

    async fn execute_once_using_read_mode(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
        http: &wreq::Client,
        retain_api_retry_after: bool,
    ) -> Result<Value, Error> {
        self.execute_once_using_decode_mode(
            req,
            structured_block_response,
            http,
            retain_api_retry_after,
            None,
        )
        .await
    }

    async fn execute_once_using_decode_mode(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
        http: &wreq::Client,
        retain_api_retry_after: bool,
        ordered_keys: Option<&mut Option<Vec<String>>>,
    ) -> Result<Value, Error> {
        self.execute_once_using_observed_decode_mode(
            req,
            structured_block_response,
            http,
            retain_api_retry_after,
            ordered_keys,
            None,
            None,
        )
        .await
    }

    async fn execute_once_using_observed_decode_mode(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
        http: &wreq::Client,
        retain_api_retry_after: bool,
        ordered_keys: Option<&mut Option<Vec<String>>>,
        receipt: Option<&ChannelReadReceipt>,
        original_bytes: Option<&mut Option<String>>,
    ) -> Result<Value, Error> {
        self.execute_once_using_field_decode_mode(
            req,
            structured_block_response,
            http,
            retain_api_retry_after,
            ordered_keys,
            receipt,
            original_bytes,
            "results",
        )
        .await
    }

    async fn execute_once_using_field_decode_mode(
        &self,
        req: &PreparedRequest,
        structured_block_response: bool,
        http: &wreq::Client,
        retain_api_retry_after: bool,
        ordered_keys: Option<&mut Option<Vec<String>>>,
        receipt: Option<&ChannelReadReceipt>,
        original_bytes: Option<&mut Option<String>>,
        ordered_field: &str,
    ) -> Result<Value, Error> {
        self.http()?;
        let req_url = req.url.clone();
        let mut builder = match req.method {
            "POST" => http.post(&req.url),
            "GET" => http.get(&req.url),
            other => return Err(Error::Transport(format!("unsupported method {other}"))),
        };
        builder = builder.timeout(API_TIMEOUT);

        for (key, value) in &req.headers {
            if key.eq_ignore_ascii_case("content-type") {
                continue;
            }
            builder = builder.header(key.as_str(), value.as_str());
        }
        builder = builder.header("Cookie", self.cookie());

        builder = match &req.body {
            RequestBody::Form(fields) => builder.form(fields),
            RequestBody::Json(value) => builder.json(value),
        };

        let response = if let Some(receipt) = receipt {
            let request = builder
                .build()
                .map_err(|_| Error::Offline("channel read request was not built".into()))?;
            receipt.observe(http.execute(request)).await
        } else {
            builder.send().await
        }
        .map_err(|error| {
            let error = if structured_block_response {
                error.without_uri()
            } else {
                error
            };
            let error = send_error(error);
            if error.is_offline() {
                self.set_health(Health::Offline);
            }
            error
        })?;
        // Slack answered. Even a 500 or a rate limit means the link is up, so
        // the rail stops saying "connecting" as soon as anything lands.
        self.set_health(Health::Online);

        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            if !structured_block_response {
                tracing::warn!(
                    url = %redact_secrets(&req_url),
                    status = %status,
                    retry_after_secs = ?retry_after_secs,
                    "slack http error"
                );
            }
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            });
        }

        let value: Value = if let Some(keys) = ordered_keys {
            let ordered = if ordered_field == "results" {
                directory_response::read_body(response, status).await?
            } else {
                directory_response::read_body_field(response, status, ordered_field).await?
            };
            *keys = ordered.keys;
            ordered.raw
        } else if let Some(original_bytes) = original_bytes {
            let bytes = response
                .bytes()
                .await
                .map_err(|_| Error::Transport("invalid split response bytes".into()))?;
            // This seam promises original bytes, so charset conversion and
            // replacement of invalid UTF8 must never precede observation.
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| Error::Transport("invalid original response UTF8".into()))?;
            let value = serde_json::from_slice(&bytes)
                .map_err(|_| Error::Transport("invalid split response JSON".into()))?;
            *original_bytes = Some(text.to_owned());
            value
        } else if url::Url::parse(&req.url).is_ok_and(|url| url.path() == "/api/blocks.suggestions")
        {
            read_suggestions_body(response).await?
        } else {
            response.json().await.map_err(|e| {
                if structured_block_response {
                    Error::Transport("invalid block interaction JSON response".into())
                } else {
                    Error::Transport(format!("decode ({status}): {e}"))
                }
            })?
        };

        // This narrow read owner must retain the actual header even when Slack
        // reports a rate limit inside a successful HTTP response.
        if retain_api_retry_after
            && !structured_block_response
            && (file_share_read::rate_limited(req, &value)
                || directory_usergroup_info::rate_limited(req, &value))
        {
            return Err(Error::RateLimited { retry_after_secs });
        }

        if structured_block_response {
            return if value.is_object() {
                Ok(value)
            } else {
                Err(Error::Transport(
                    "invalid block interaction response object".into(),
                ))
            };
        }

        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let err = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    if req_url.contains("/api/conversations.leave?")
                        && value.get("not_in_channel").and_then(Value::as_bool) == Some(true)
                    {
                        "not_in_channel"
                    } else {
                        "unknown_error"
                    }
                })
                .to_owned();
            tracing::warn!(
                url = %redact_secrets(&req_url),
                status = %status,
                body = %redact_secrets(&value.to_string()),
                "slack api error"
            );
            return Err(Error::Api(err));
        }

        Ok(value)
    }

    pub async fn get_text(&self, url: &str, user_agent: &str) -> Result<String, Error> {
        let response = self
            .http()?
            .get(url)
            .header("User-Agent", user_agent)
            .header("Cookie", self.cookie())
            .send()
            .await
            .map_err(send_error)?;
        response
            .text()
            .await
            .map_err(|e| Error::Transport(format!("read body: {e}")))
    }

    pub async fn get_bytes(&self, url: &str, user_agent: &str) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, true, None).await
    }

    pub async fn get_public_bytes(&self, url: &str, user_agent: &str) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, false, None).await
    }

    pub async fn get_public_gif_bytes(
        &self,
        url: &str,
        user_agent: &str,
    ) -> Result<Vec<u8>, Error> {
        self.get_bytes_with_auth(url, user_agent, false, Some("image/gif"))
            .await
    }

    async fn get_bytes_with_auth(
        &self,
        url: &str,
        user_agent: &str,
        authenticated: bool,
        accept: Option<&str>,
    ) -> Result<Vec<u8>, Error> {
        // Media never waits: loads are serialized behind one lock, so holding
        // here would park every later image behind a link that may be gone for
        // minutes. Failing now is free — the loader retries on its own delay.
        if self.health() == Health::Offline {
            return Err(Error::Offline("network is down".into()));
        }
        // Media hosts are third-party and occasionally never answer. Without a
        // bound, one stalled avatar or unfurl image wedges the whole media
        // refresh (and every later one, since loads are serialized).
        let mut request = self
            .http()?
            .get(url)
            .timeout(MEDIA_FETCH_TIMEOUT)
            .header("User-Agent", user_agent);
        if let Some(accept) = accept {
            request = request.header("Accept", accept);
        }
        if authenticated {
            request = request.header("Cookie", self.cookie());
        }
        let response = request.send().await.map_err(send_error)?;

        let status = response.status();
        let retry_after_secs = retry_after_secs(response.headers());
        if status.as_u16() == 429 {
            return Err(Error::RateLimited { retry_after_secs });
        }
        if !status.is_success() {
            tracing::warn!(
                url = %redact_secrets(url),
                status = %status,
                retry_after_secs = ?retry_after_secs,
                "slack file http error"
            );
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                retry_after_secs,
            });
        }

        read_media_body(response, MAX_MEDIA_BYTES).await
    }
}

fn is_block_interaction_request(req: &PreparedRequest) -> bool {
    // Calendar has one captured scalar preference write. Never grant this
    // single-attempt seam to unrelated preferences or arbitrary form members.
    if req.method == "POST"
        && url::Url::parse(&req.url).is_ok_and(|u| u.path() == "/api/users.prefs.set")
    {
        let RequestBody::Form(fields) = &req.body else {
            return false;
        };
        let map: std::collections::BTreeMap<_, _> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        return map.len() == fields.len()
            && map.len() == 4
            && map.get("token").is_some_and(|s| !s.is_empty())
            && map.get("name") == Some(&"calendar_share_event_in_channel")
            && matches!(map.get("value"), Some(&"true" | &"false"))
            && map.get("_x_reason") == Some(&"prefs-api/setUserPrefByApi");
    }
    req.method == "POST"
        && url::Url::parse(&req.url).is_ok_and(|url| {
            matches!(
                url.path(),
                "/api/blocks.actions"
                    | "/api/apps.actions.v2.execute"
                    | "/api/apps.home.dispatchOpenEvent"
                    | "/api/calendar.event.create"
                    | "/api/rooms.request"
                    | "/api/apps.home.open"
                    | "/api/apps.user.connection.submit"
                    | "/api/apps.auth.external.start"
                    | "/api/workflows.triggers.trip"
                    | "/api/apps.auth.external.authMapping.update"
                    | "/api/sfdc.integration.listOrgs"
                    | "/api/sfdc.integration.authUser"
                    | "/api/sfdc.integration.auth.loginUrl"
                    | "/api/sfdc.integration.disconnectUser"
                    | "/api/blocks.suggestions"
                    | "/api/views.get"
                    | "/api/views.submit"
                    | "/api/views.close"
            )
        })
}

#[cfg(test)]
#[path = "transport/block_interaction_tests.rs"]
mod block_interaction_tests;

#[cfg(test)]
#[path = "transport/home_workflow_tests.rs"]
mod home_workflow_tests;

async fn read_suggestions_body(response: wreq::Response) -> Result<Value, Error> {
    let limit = crate::block_actions::external::MAX_SUGGESTIONS_BYTES;
    let invalid = || Error::Transport("invalid or excessive suggestions response".into());
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| invalid())?;
        if chunk.len() > limit - bytes.len() {
            return Err(invalid());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

async fn read_media_body(response: wreq::Response, limit: usize) -> Result<Vec<u8>, Error> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(Error::Transport("media body exceeds size limit".into()));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Transport(format!("read body: {e}")))?;
        if chunk.len() > limit - bytes.len() {
            return Err(Error::Transport("media body exceeds size limit".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub fn retry_after_secs(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// A dropped link surfaces the same way on every call in flight: a refused or
/// unroutable connect, a name that will not resolve, a timeout, or a socket the
/// peer reset. Naming that class here is what lets the shell answer it with the
/// connection indicator rather than a toast full of signed URL.
fn send_error(error: wreq::Error) -> Error {
    if error.is_connect() || error.is_timeout() || error.is_connection_reset() || error.is_request()
    {
        return Error::Offline(error.without_uri().to_string());
    }
    Error::Transport(format!("send: {error}"))
}

fn retryable_error(error: &Error) -> bool {
    // A link that just came back hands out one more failure before it settles:
    // the pooled socket is dead, or DNS has not caught up. Retrying re-enters
    // the hold, so the second attempt waits for the link to be confirmed again
    // rather than firing into the same gap.
    error.is_offline()
        || matches!(error, Error::RateLimited { .. })
        || matches!(
            error,
            Error::HttpStatus {
                status: 500..=599,
                ..
            }
        )
}

fn retry_delay(error: &Error, attempt: u32) -> Duration {
    let fallback_ms = 250 * 2_u64.saturating_pow(attempt.saturating_sub(1));
    let secs = match error {
        Error::RateLimited {
            retry_after_secs: Some(secs),
        }
        | Error::HttpStatus {
            retry_after_secs: Some(secs),
            ..
        } => Some((*secs).min(30)),
        _ => None,
    };
    secs.map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_millis(fallback_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use wreq::header::HeaderValue;

    #[test]
    fn parses_retry_after_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("12"));
        assert_eq!(retry_after_secs(&headers), Some(12));
    }

    #[test]
    fn ignores_invalid_retry_after() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("soon"));
        assert_eq!(retry_after_secs(&headers), None);
    }

    #[test]
    fn retry_policy_is_bounded() {
        assert!(retryable_error(&Error::RateLimited {
            retry_after_secs: Some(60)
        }));
        assert!(
            retry_delay(
                &Error::RateLimited {
                    retry_after_secs: Some(60)
                },
                1
            ) <= Duration::from_secs(30)
        );
        assert!(retryable_error(&Error::HttpStatus {
            status: 503,
            retry_after_secs: None
        }));
        assert!(!retryable_error(&Error::HttpStatus {
            status: 404,
            retry_after_secs: None
        }));
        assert!(retryable_error(&Error::Offline(
            "client error (Connect)".into()
        )));
    }

    /// A refused connect is the shell's offline signal, and the transport has
    /// to leave the health cell saying so — that mark is what keeps a raw URL
    /// out of the toast strip and puts a spinner on the rail instead.
    #[tokio::test]
    async fn a_refused_connect_reads_as_offline_and_marks_the_health_cell() {
        // Bound and immediately dropped: nothing is listening on this port, so
        // the connect is refused rather than left hanging.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);

        let transport = Transport::new("secret-cookie").unwrap();
        assert_eq!(transport.health(), Health::Unknown);
        let request = PreparedRequest {
            method: "POST",
            url: format!("http://{address}/api/client.dms?token=secret"),
            headers: Vec::new(),
            body: RequestBody::Form(Vec::new()),
        };

        let error = transport.execute(request).await.unwrap_err();
        assert!(error.is_offline(), "{error}");
        assert_eq!(transport.health(), Health::Offline);

        // And the signed URL never rides along in the message.
        assert!(!error.to_string().contains("token=secret"), "{error}");
    }

    /// Requests are held, not fired, while the link is known to be down — and
    /// released the moment recovery is confirmed. Firing anyway is what left
    /// panes stuck on "Loading…" behind a request that could never land.
    #[tokio::test]
    async fn a_call_is_held_while_the_link_is_down_and_released_when_it_returns() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
                )
                .await
                .unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let held = tokio::spawn({
            let transport = transport.clone();
            async move {
                transport
                    .execute(PreparedRequest {
                        method: "POST",
                        url: format!("http://{address}/api/client.dms"),
                        headers: Vec::new(),
                        body: RequestBody::Form(Vec::new()),
                    })
                    .await
            }
        });

        // Nothing has been sent: the server is still waiting to be accepted.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!held.is_finished(), "the call went out over a dead link");

        // The shell confirms the network is back.
        transport.set_health(Health::Unknown);
        let value = tokio::time::timeout(Duration::from_secs(5), held)
            .await
            .expect("released")
            .unwrap()
            .unwrap();
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        server.await.unwrap();
    }

    /// The whole cycle a dropped link puts a call through: cut loose mid-flight
    /// rather than left to time out, held until recovery is confirmed, then
    /// sent again. The pane behind it fills in instead of staying on "Loading…".
    #[tokio::test]
    async fn a_call_cut_loose_by_a_drop_is_held_and_then_sent_again() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            // The first connection is the one the drop cuts loose: read the
            // request, answer nothing.
            let (mut dead, _) = listener.accept().await.unwrap();
            dead.read(&mut vec![0; 8192]).await.unwrap();
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.read(&mut vec![0; 8192]).await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}\r\n",
                )
                .await
                .unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        let call = tokio::spawn({
            let transport = transport.clone();
            async move {
                transport
                    .execute(PreparedRequest {
                        method: "GET",
                        url: format!("http://{address}/api/conversations.history?"),
                        headers: Vec::new(),
                        body: RequestBody::Form(Vec::new()),
                    })
                    .await
            }
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        transport.set_health(Health::Offline);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!call.is_finished(), "gave up instead of holding");

        transport.set_health(Health::Unknown);
        let value = tokio::time::timeout(Duration::from_secs(5), call)
            .await
            .expect("released well inside the request timeout")
            .unwrap()
            .unwrap();
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        server.await.unwrap();
    }

    /// The hold is bounded. A confirmation that never comes must not turn into
    /// the very hang this path exists to remove.
    #[tokio::test(start_paused = true)]
    async fn a_held_call_gives_up_rather_than_hanging_forever() {
        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let error = transport
            .execute(PreparedRequest {
                method: "POST",
                url: "http://127.0.0.1:1/api/client.dms".to_owned(),
                headers: Vec::new(),
                body: RequestBody::Form(Vec::new()),
            })
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");
    }

    /// Media is the exception: loads are serialized, so holding one would park
    /// every later image behind it. It fails immediately instead, without
    /// putting a doomed request on the wire.
    #[tokio::test]
    async fn a_media_fetch_fails_immediately_instead_of_holding_the_loader() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let transport = Transport::new("secret-cookie").unwrap();
        transport.set_health(Health::Offline);
        let error = transport
            .get_bytes(
                &format!("http://{address}/avatar.png"),
                "super-platinum-test",
            )
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");

        let accepted = tokio::time::timeout(Duration::from_millis(150), listener.accept()).await;
        assert!(accepted.is_err(), "a doomed fetch still went out");
    }

    async fn captured_image_request(public: bool, gif: bool) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let read = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        let transport = Transport::new("secret-cookie").unwrap();
        let url = format!("http://{address}/image.{}", if gif { "gif" } else { "png" });
        let bytes = if gif {
            transport
                .get_public_gif_bytes(&url, "super-platinum-test")
                .await
        } else if public {
            transport
                .get_public_bytes(&url, "super-platinum-test")
                .await
        } else {
            transport.get_bytes(&url, "super-platinum-test").await
        }
        .unwrap();
        assert_eq!(bytes, b"ok");
        server.await.unwrap()
    }

    #[tokio::test]
    async fn media_read_rejects_large_declared_and_streamed_bodies() {
        for (headers, body) in [
            ("Content-Length: 1000\r\n", ""),
            (
                "Transfer-Encoding: chunked\r\n",
                "4\r\nabcd\r\n4\r\nefgh\r\n0\r\n\r\n",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let reply = format!("HTTP/1.1 200 OK\r\n{headers}Connection: close\r\n\r\n{body}");
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                socket.read(&mut [0; 8192]).await.unwrap();
                let _ = socket.write_all(reply.as_bytes()).await;
            });
            let transport = Transport::new("cookie").unwrap();
            let response = transport
                .http()
                .unwrap()
                .get(format!("http://{address}/image"))
                .send()
                .await
                .unwrap();
            let error = read_media_body(response, 5).await.unwrap_err();
            assert!(error.to_string().contains("size limit"), "{error}");
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn public_image_fetch_never_sends_slack_cookie() {
        let request = captured_image_request(true, false).await;
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
    }

    #[tokio::test]
    async fn public_gif_fetch_forces_gif_content_negotiation_without_cookie() {
        let request = captured_image_request(true, true)
            .await
            .to_ascii_lowercase();
        assert!(request.contains("accept: image/gif"));
        assert!(!request.contains("cookie:"));
    }

    #[tokio::test]
    async fn slack_image_fetch_sends_session_cookie() {
        let request = captured_image_request(false, false).await;
        assert!(
            request
                .to_ascii_lowercase()
                .contains("cookie: d=secret-cookie")
        );
    }

    #[tokio::test]
    async fn image_fetch_follows_cross_host_redirect_without_forwarding_cookie() {
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination_address = destination.local_addr().unwrap();
        let destination_server = tokio::spawn(async move {
            let (mut socket, _) = destination.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let read = socket.read(&mut request).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });
        let redirect = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_address = redirect.local_addr().unwrap();
        let redirect_server = tokio::spawn(async move {
            let (mut socket, _) = redirect.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let _ = socket.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/avatar.png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let transport = Transport::new("secret-cookie").unwrap();
        let bytes = transport
            .get_bytes(
                &format!("http://{redirect_address}/avatar"),
                "super-platinum-test",
            )
            .await
            .unwrap();
        assert_eq!(bytes, b"ok");
        redirect_server.await.unwrap();
        let destination_request = destination_server.await.unwrap().to_ascii_lowercase();
        assert!(!destination_request.contains("cookie:"));
    }
}

#[cfg(any(test, feature = "protected-test-support"))]
mod disconnected_fixture;

#[cfg(test)]
#[path = "transport/calendar_tests.rs"]
mod calendar_tests;

#[cfg(test)]
#[path = "transport/home_navigation_tests.rs"]
mod home_navigation_tests;

mod list_board_choice;
pub use list_board_choice::ListBoardChoiceFailure;

mod list_board_group_delete;

mod list_view_refinement_save;
pub use list_view_refinement_save::ListViewRefinementSaveFailure;

mod list_default_view_preference;
pub use list_default_view_preference::ListDefaultViewPreferenceFailure;
