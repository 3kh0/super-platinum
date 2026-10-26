//! Explicit caller-owned application presentation, not an observed Slack account
//! locale. Retained laT0 uses canvas/Untitled; Fbwy returns the source literal
//! for en-US before consulting translation bundles. Other consumers must supply
//! their own observation. No shared/global default or locale catalog is created.
use serde_json::{Value, json};

/// Application-owned English spelling for a known private nonmember channel.
/// This neither observes account locale nor authorizes absent/unknown rows.
#[derive(Clone, Copy)]
pub(crate) struct ChannelTitle;
impl ChannelTitle {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) const fn context(
        self,
    ) -> super_platinum_core::slack::api::custom::texty::file_titles::PrivateChannelText {
        super_platinum_core::slack::api::custom::texty::file_titles::PrivateChannelText::EnglishPlaceholder
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"channel_token","source":"private-channel",
            "value":"private-channel","origin":"application-presentation"})
    }
    pub(crate) const fn observed_context(
        self,
    ) -> super_platinum_core::slack::api::custom::texty::file_titles::ChannelText {
        super_platinum_core::slack::api::custom::texty::file_titles::ChannelText::EnglishPlaceholders
    }
    pub(crate) fn observed_descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"channel_token","unknown":"unknown-channel","nonexistent":"private-channel","origin":"application-presentation"})
    }
}

/// Independent English emoji text presentation, not a claim about the user's
/// Slack locale or its translation map. The retained canonical intersection is
/// static; source/account custody remains with each file observation.
#[derive(Clone, Copy)]
pub(crate) struct EmojiTitle;
impl EmojiTitle {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) const fn context(
        self,
    ) -> super_platinum_core::slack::api::custom::texty::file_titles::EmojiText {
        super_platinum_core::slack::api::custom::texty::file_titles::EmojiText::EnglishCanonicalNames
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","format":"canonical-colon-name",
            "catalog":"retained-v16-v17-canonical-intersection","origin":"application-presentation"})
    }
    pub(crate) fn proof(self, current: bool) -> Value {
        json!({"current":current,"descriptor":self.descriptor()})
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CanvasTitle;
impl CanvasTitle {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) const fn value(self) -> &'static str {
        "Untitled"
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"canvas","source":"Untitled",
            "value":self.value(),"origin":"application-presentation"})
    }
    pub(crate) fn proof(self, current: bool) -> Value {
        json!({"current":current,"descriptor":self.descriptor()})
    }
}

/// The application's English group token spelling, separate from account locale.
#[derive(Clone, Copy)]
pub(crate) struct GroupTitle;
impl GroupTitle {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) fn context(
        self,
    ) -> super_platinum_core::slack::api::custom::texty::file_titles::GroupTitle {
        super_platinum_core::slack::api::custom::texty::file_titles::GroupTitle::english()
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"usergroup","value":"unknown-group","origin":"application-presentation"})
    }
}

/// Explicit English Canvas status presentation; source status remains separately owned.
#[derive(Clone, Copy)]
pub(crate) struct PrivateCanvas;
impl PrivateCanvas {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) fn value(self) -> &'static str {
        super_platinum_core::slack::api::custom::texty::file_titles::CanvasReferenceText::english()
            .private_canvas()
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"canvas","source":"Private canvas","value":self.value(),
            "origin":"application-presentation","token":{"kind":"private-canvas","icon":"lock","disabled":false}})
    }
}

/// Explicit English Canvas status presentation; source status remains separately owned.
#[derive(Clone, Copy)]
pub(crate) struct DeletedCanvas;
impl DeletedCanvas {
    pub(crate) const fn english() -> Self {
        Self
    }
    pub(crate) fn value(self) -> &'static str {
        super_platinum_core::slack::api::custom::texty::file_titles::CanvasReferenceText::english()
            .deleted_canvas()
    }
    pub(crate) fn descriptor(self) -> Value {
        json!({"locale":"en-US","namespace":"canvas","source":"Deleted canvas","value":self.value(),
            "origin":"application-presentation","token":{"kind":"deleted-canvas","icon":"delete","disabled":true}})
    }
}
