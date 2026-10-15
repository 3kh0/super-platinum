//! One typed registry/dispatch path for column and saved-view names.
use super::*;
use super_platinum_core::slack_lists::{
    PersonalTaskLists, ViewCreate, ViewCreateAttempt, ViewCreatePermit, ViewLayout, ViewRename,
    ViewRenameAttempt, ViewRenamePermit,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Column(String),
    View(String),
    NewView(ViewLayout),
}
impl Target {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Column(v) | Self::View(v) => Some(v),
            Self::NewView(_) => None,
        }
    }
    pub fn creation(&self) -> Option<ViewLayout> {
        if let Self::NewView(layout) = self {
            Some(*layout)
        } else {
            None
        }
    }
    pub fn is_view(&self) -> bool {
        matches!(self, Self::View(_) | Self::NewView(_))
    }
    pub fn noun(&self) -> &'static str {
        if self.is_view() { "view" } else { "field" }
    }
    pub fn limit(&self) -> usize {
        if self.is_view() { 60 } else { 256 }
    }
    pub fn name<'a>(&self, doc: &'a Document) -> Option<&'a str> {
        match self {
            Self::NewView(_) => Some(""),
            Self::Column(id) => doc.column(id).map(|c| c.name.as_str()),
            Self::View(id) => doc
                .views
                .iter()
                .find(|v| v["id"] == id.as_str())?
                .get("name")?
                .as_str(),
        }
    }
    pub fn supports(&self, doc: &Document) -> bool {
        match self {
            Self::NewView(_) => ViewCreate::supports(doc),
            Self::Column(id) => doc.column(id).is_some_and(|c| {
                c.kind == "text" && c.raw.get("options") == Some(&json!({"format":"text"}))
            }),
            Self::View(id) => doc
                .views
                .iter()
                .find(|v| v["id"] == id.as_str())
                .is_some_and(ViewRename::supports),
        }
    }
    pub fn capture(
        &self,
        doc: &Document,
        name: &str,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Intent, super_platinum_core::slack_lists::ListError> {
        match self {
            Self::NewView(layout) => {
                ViewCreate::capture(doc, name, *layout, allow, personal).map(Intent::Create)
            }
            Self::Column(id) => {
                ColumnRename::capture(doc, id, name, allow, personal).map(Intent::Column)
            }
            Self::View(id) => ViewRename::capture(doc, id, name, allow, personal).map(Intent::View),
        }
    }
}
#[derive(Clone)]
pub(super) enum Intent {
    Column(ColumnRename),
    View(ViewRename),
    Create(ViewCreate),
}
impl Intent {
    pub fn original(&self) -> &Document {
        match self {
            Self::Column(i) => i.original(),
            Self::View(i) => i.original(),
            Self::Create(i) => i.original(),
        }
    }
    pub fn desired_name(&self) -> &str {
        match self {
            Self::Column(i) => i.desired_name(),
            Self::View(i) => i.desired_name(),
            Self::Create(i) => i.desired_name(),
        }
    }
    pub fn observes_desired(&self, doc: &Document) -> bool {
        match self {
            Self::Column(i) => i.observes_desired(doc),
            Self::View(i) => i.observes_desired(doc),
            Self::Create(_) => false, // A matching new view is never causal proof.
        }
    }
    pub fn admit(
        &self,
        doc: &Document,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
    ) -> Option<Permit> {
        match self {
            Self::Create(i) => ViewCreateAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Create),
            Self::Column(i) => ColumnRenameAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Column),
            Self::View(i) => ViewRenameAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::View),
        }
    }
}
pub(super) enum Permit {
    Column(ColumnRenamePermit),
    View(ViewRenamePermit),
    Create(ViewCreatePermit),
}
impl Permit {
    pub fn is_creation(&self) -> bool {
        matches!(self, Self::Create(_))
    }
    pub fn is_view(&self) -> bool {
        matches!(self, Self::View(_))
    }
    pub fn request(
        self,
        client: &super_platinum_core::slack::SlackClient,
        session: &super_platinum_core::config::WorkspaceSession,
    ) -> super_platinum_core::slack::client::PreparedRequest {
        match self {
            Self::Column(p) => {
                super_platinum_core::slack::api::list_column_rename(client, session, p)
            }
            Self::View(p) => super_platinum_core::slack::api::list_view_rename(client, session, p),
            Self::Create(p) => {
                super_platinum_core::slack::api::list_view_create(client, session, p)
            }
        }
    }
}
