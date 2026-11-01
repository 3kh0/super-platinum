//! Exact timestamp-purpose read descriptions; no native/Copy/pane capability.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    History,
    Channel,
}

// Neither the request nor its authenticated PreparedRequest is publicly
// constructible/Debug/serializable. Root IO can inspect only inert coordinates.
pub struct Request {
    pub(in crate::slack) inner: ApiRequest,
    kind: Kind,
    channel: String,
    timestamp: String,
}
pub(in crate::slack) struct ApiRequest {
    pub(in crate::slack) prepared: PreparedRequest,
}
impl Request {
    fn new(
        client: &SlackClient,
        session: &WorkspaceSession,
        channel: &str,
        ts: &str,
        kind: Kind,
    ) -> Result<Self, Error> {
        if !crate::thread_timestamp::channel_id(channel)
            || !crate::thread_timestamp::timestamp(ts)
            || session.token.is_empty()
            || session.token.len() > 16 * 1024
            || session
                .token
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(Error::Api(
                "timestamp_navigation_request_unavailable".into(),
            ));
        }
        let (method, fields) = match kind {
            Kind::History => (
                "conversations.history",
                vec![
                    ("channel", channel.into()),
                    ("latest", ts.into()),
                    ("inclusive", "true".into()),
                    ("limit", "1".into()),
                ],
            ),
            Kind::Channel => (
                "conversations.info",
                vec![
                    ("channel", channel.into()),
                    ("include_num_members", "false".into()),
                ],
            ),
        };
        let prepared = client.rest_form(session, method, fields);
        let url = url::Url::parse(&prepared.url)
            .map_err(|_| Error::Api("timestamp_navigation_request_unavailable".into()))?;
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
            || url.as_str() != prepared.url
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path() != format!("/api/{method}")
        {
            return Err(Error::Api(
                "timestamp_navigation_request_unavailable".into(),
            ));
        }
        Ok(Self {
            inner: ApiRequest { prepared },
            kind,
            channel: channel.into(),
            timestamp: ts.into(),
        })
    }
    pub fn history(
        client: &SlackClient,
        session: &WorkspaceSession,
        channel: &str,
        ts: &str,
    ) -> Result<Self, Error> {
        Self::new(client, session, channel, ts, Kind::History)
    }
    pub fn channel(
        client: &SlackClient,
        session: &WorkspaceSession,
        channel: &str,
        ts: &str,
    ) -> Result<Self, Error> {
        Self::new(client, session, channel, ts, Kind::Channel)
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn channel_id(&self) -> &str {
        &self.channel
    }
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
}
