use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTextField {
    Name,
    Topic,
    Description,
}
impl ChannelTextField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Topic => "topic",
            Self::Description => "purpose",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Channel name",
            Self::Topic => "Topic",
            Self::Description => "Description",
        }
    }
    pub fn value(self, channel: &Channel) -> String {
        if self == Self::Name {
            return channel.name.clone().unwrap_or_default();
        }
        channel
            .extra
            .get(self.key())
            .and_then(|v| {
                v.as_str()
                    .or_else(|| v.get("value").and_then(|v| v.as_str()))
            })
            .unwrap_or_default()
            .into()
    }
}
#[derive(Debug, Default, serde::Deserialize)]
pub struct ConversationMembersPage {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub response_metadata: Option<super::super::models::ResponseMetadata>,
    #[serde(default)]
    pub users: Vec<User>,
}
#[derive(Debug, Default, Clone)]
pub struct ConversationMembers {
    pub ids: Vec<String>,
    pub cursor: Option<String>,
    seen: std::collections::BTreeSet<String>,
    cursors: std::collections::BTreeSet<String>,
}
impl ConversationMembers {
    pub fn confirm_member(&mut self, id: &str, present: bool) {
        if present {
            if self.seen.insert(id.to_owned()) {
                self.ids.push(id.to_owned());
            }
        } else {
            self.seen.remove(id);
            self.ids.retain(|member| member != id);
        }
    }
    /// Returns false if Slack repeats a cursor. Keep loaded members and stop
    /// pagination instead of issuing the same request indefinitely.
    pub fn apply(&mut self, page: ConversationMembersPage) -> bool {
        for id in page.members {
            if !id.is_empty() && self.seen.insert(id.clone()) {
                self.ids.push(id);
            }
        }
        self.cursor = page
            .response_metadata
            .and_then(|m| m.next_cursor)
            .filter(|s| !s.trim().is_empty());
        if let Some(cursor) = &self.cursor {
            if !self.cursors.insert(cursor.clone()) {
                self.cursor = None;
                return false;
            }
        }
        true
    }
}
pub fn conversation_members(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    cursor: Option<&str>,
) -> PreparedRequest {
    let mut fields = vec![("channel", channel.into()), ("limit", "100".into())];
    if let Some(cursor) = cursor.filter(|s| !s.is_empty()) {
        fields.push(("cursor", cursor.into()));
    }
    client.rest_form(workspace, "conversations.members", fields)
}
pub async fn fetch_conversation_members(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    cursor: Option<&str>,
) -> Result<ConversationMembersPage, Error> {
    if workspace.enterprise_id.is_some() || workspace.team_id.starts_with('E') {
        #[derive(serde::Deserialize)]
        struct EdgePage {
            #[serde(default)]
            results: Vec<User>,
            #[serde(default)]
            next_marker: Option<String>,
        }
        let request = conversation_members_edge(client, workspace, channel, cursor, 100)
            .map_err(|_| Error::Transport("Unable to build channel members request".into()))?;
        let page: EdgePage = decode(transport.execute(request).await?, "channel members")?;
        return Ok(ConversationMembersPage {
            members: page.results.iter().map(|u| u.id.clone()).collect(),
            users: page.results,
            response_metadata: Some(super::super::models::ResponseMetadata {
                next_cursor: page.next_marker,
                ..Default::default()
            }),
        });
    }
    decode(
        transport
            .execute(conversation_members(client, workspace, channel, cursor))
            .await?,
        "conversation members",
    )
}
pub fn conversation_members_edge(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    marker: Option<&str>,
    count: u32,
) -> serde_json::Result<PreparedRequest> {
    let mut body = serde_json::json!({"channels":[channel],"count":count.clamp(1,100),"filter":"people","index":"users_by_display_name","locale":"en-US","present_first":false,"fuzz":1,"include_profile_only_users":true});
    if let Some(marker) = marker.filter(|s| !s.is_empty()) {
        body["marker"] = marker.into();
    }
    client.edge_json(workspace, "users/list", body)
}
pub async fn fetch_conversation_details(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
) -> Result<Channel, Error> {
    #[derive(serde::Deserialize)]
    struct Info {
        channel: Channel,
    }
    let request = client.rest_form(
        workspace,
        "conversations.info",
        vec![
            ("channel", channel.into()),
            ("include_num_members", "true".into()),
        ],
    );
    let info: Info = decode(transport.execute(request).await?, "conversation details")?;
    Ok(info.channel)
}
pub fn conversation_set_text(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    field: ChannelTextField,
    value: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        match field {
            ChannelTextField::Name => "conversations.rename",
            ChannelTextField::Topic => "conversations.setTopic",
            ChannelTextField::Description => "conversations.setPurpose",
        },
        vec![("channel", channel.into()), (field.key(), value.into())],
    )
}
pub async fn set_conversation_text(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    field: ChannelTextField,
    value: &str,
) -> Result<(), Error> {
    transport
        .execute(conversation_set_text(
            client, workspace, channel, field, value,
        ))
        .await
        .map(|_| ())
}
