use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::file_share) enum Conversation {
    Channel,
    Direct,
    NewDirect,
    Group,
}
impl Conversation {
    pub(in crate::file_share) fn from_alias(alias: &str) -> Self {
        match alias {
            "file-share-dm" | "file-share-dm-narrow" => Self::Direct,
            "file-share-new-dm" | "file-share-new-dm-narrow" => Self::NewDirect,
            "file-share-group-dm" | "file-share-group-dm-narrow" => Self::Group,
            _ => Self::Channel,
        }
    }
    pub(in crate::file_share) fn id(self) -> &'static str {
        match self {
            Self::Channel => DESTINATION,
            Self::Direct | Self::NewDirect => "D00000001",
            Self::Group => "G00000001",
        }
    }
    pub(in crate::file_share) fn raw(self) -> Value {
        match self {
            Self::Channel => destination(),
            Self::Direct | Self::NewDirect => {
                json!({"id":self.id(),"is_im":true,"user":"U00000002","is_open":true,
                "is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"future":{"keep":1}})
            }
            Self::Group => {
                json!({"id":self.id(),"name":"mpdm-you--alice--bob-1","is_mpim":true,"is_private":true,
                "members":[USER,"U00000002","U00000003"],"is_open":true,
                "is_ext_shared":false,"is_read_only":false,"is_thread_only":false,"future":{"keep":2}})
            }
        }
    }
}
pub(in crate::file_share) fn peer() -> Value {
    json!({"id":"U00000002","name":"alice","real_name":"Alice Example","deleted":false,"is_bot":false,
        "profile":{"display_name":"Alice Example"},"future":{"keep":3}})
}
impl Capability {
    pub(in crate::file_share) fn destination_id(&self) -> &'static str {
        self.0.conversation.id()
    }
    pub(in crate::file_share) async fn peer_info(&self, id: &str) -> Result<Value, Error> {
        if !matches!(
            self.0.conversation,
            Conversation::Direct | Conversation::NewDirect
        ) || id != "U00000002"
        {
            return Err(rejected(Rejected::Read));
        }
        self.read("users/info").map_err(rejected)?;
        Ok(json!({"ok":true,"results":[peer()],"failed_ids":[]}))
    }
    #[cfg(test)]
    pub(in crate::file_share) fn with_conversation(mut self, conversation: Conversation) -> Self {
        // Only before installing or cloning the closed capability in test seed.
        Arc::get_mut(&mut self.0).unwrap().conversation = conversation;
        self
    }
}
