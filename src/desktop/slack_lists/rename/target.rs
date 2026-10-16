//! One typed registry/dispatch path for names and Number display precision.
use super::*;
use super_platinum_core::slack_lists::{ColumnCreate, ColumnCreatePermit};
use super_platinum_core::slack_lists::{
    ColumnPrecision, ColumnPrecisionAttempt, ColumnPrecisionPermit, PersonalTaskLists, ViewCreate,
    ViewCreateAttempt, ViewCreatePermit, ViewFieldVisibility, ViewFieldVisibilityAttempt,
    ViewFieldVisibilityPermit, ViewLayout, ViewRename, ViewRenameAttempt, ViewRenamePermit,
};
use super_platinum_core::slack_lists::{
    ViewFilter, ViewFilterAttempt, ViewFilterChoice, ViewFilterPermit,
};
use super_platinum_core::slack_lists::{
    ViewGrouping, ViewGroupingAttempt, ViewGroupingChoice, ViewGroupingPermit,
};
use super_platinum_core::slack_lists::{ViewSort, ViewSortAttempt, ViewSortChoice, ViewSortPermit};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Column(String),
    NewTextField(String),
    Precision(String),
    View(String),
    Filter(String),
    Sort(String),
    Grouping(String),
    Visibility { view: String, column: String },
    NewView(ViewLayout, Option<String>),
}
impl Target {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::NewTextField(v)
            | Self::Column(v)
            | Self::Precision(v)
            | Self::View(v)
            | Self::Sort(v)
            | Self::Grouping(v)
            | Self::Filter(v) => Some(v),
            Self::NewView(_, _) => None,
            Self::Visibility { view, .. } => Some(view),
        }
    }
    pub fn is_text_creation(&self) -> bool {
        matches!(self, Self::NewTextField(_))
    }
    pub fn is_grouping(&self) -> bool {
        matches!(self, Self::Grouping(_))
    }
    pub fn is_sort(&self) -> bool {
        matches!(self, Self::Sort(_))
    }
    pub fn is_filter(&self) -> bool {
        matches!(self, Self::Filter(_))
    }
    pub fn is_visibility(&self) -> bool {
        matches!(self, Self::Visibility { .. })
    }
    pub fn is_precision(&self) -> bool {
        matches!(self, Self::Precision(_))
    }
    pub fn creation(&self) -> Option<ViewLayout> {
        if let Self::NewView(layout, _) = self {
            Some(*layout)
        } else {
            None
        }
    }
    pub fn grouping(&self) -> Option<&str> {
        match self {
            Self::NewView(_, key) => key.as_deref(),
            _ => None,
        }
    }
    pub fn is_view(&self) -> bool {
        matches!(
            self,
            Self::NewTextField(_)
                | Self::View(_)
                | Self::Sort(_)
                | Self::Grouping(_)
                | Self::Filter(_)
                | Self::NewView(_, _)
                | Self::Visibility { .. }
        )
    }
    pub fn noun(&self) -> &'static str {
        if self.is_view() && !self.is_text_creation() {
            "view"
        } else {
            "field"
        }
    }
    pub fn limit(&self) -> usize {
        if self.is_view() && !self.is_text_creation() {
            60
        } else {
            256
        }
    }
    pub fn name<'a>(&self, doc: &'a Document) -> Option<&'a str> {
        match self {
            Self::NewView(_, _) | Self::NewTextField(_) => Some(""),
            Self::Visibility { column, .. } => doc.column(column).map(|c| c.name.as_str()),
            Self::Column(id) | Self::Precision(id) => doc.column(id).map(|c| c.name.as_str()),
            Self::View(id) | Self::Sort(id) | Self::Filter(id) | Self::Grouping(id) => doc
                .views
                .iter()
                .find(|v| v["id"] == id.as_str())?
                .get("name")?
                .as_str(),
        }
    }
    pub fn supports(&self, doc: &Document) -> bool {
        match self {
            Self::NewTextField(id) => doc
                .views
                .iter()
                .any(|v| v["id"] == id.as_str() && v["type"] == "table" && ViewRename::supports(v)),
            Self::NewView(_, _) => ViewCreate::supports(doc),
            Self::Filter(view) => ViewFilter::current(doc, view).is_some(),
            Self::Sort(view) => ViewSort::current(doc, view).is_some(),
            Self::Grouping(view) => ViewGrouping::current(doc, view).is_some(),
            Self::Visibility { view, column } => {
                ViewFieldVisibility::current(doc, view, column).is_some()
            }
            Self::Precision(id) => precision::current(doc, id).is_some(),
            Self::Column(id) => doc.column(id).is_some_and(ColumnRename::supports),
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
        input: &Input,
        allow: Option<&str>,
        personal: Option<&PersonalTaskLists>,
    ) -> Result<Intent, super_platinum_core::slack_lists::ListError> {
        let name = input.name().unwrap_or_default();
        if self.is_text_creation() != matches!(input, Input::NewTextField(_))
            || self.is_filter() != matches!(input, Input::Filter(_))
            || self.is_sort() != matches!(input, Input::Sort(_))
            || self.is_grouping() != matches!(input, Input::Grouping(_))
            || self.is_precision() != matches!(input, Input::Precision(_))
            || self.is_visibility() != matches!(input, Input::Visibility(_))
        {
            return Err(super_platinum_core::slack_lists::ListError(
                "The schema editor input changed.",
            ));
        }
        match self {
            Self::NewTextField(_) => Err(super_platinum_core::slack_lists::ListError(
                "Text field creation requires its captured item population.",
            )),
            Self::Grouping(view) => {
                let Input::Grouping(choice) = input else {
                    unreachable!()
                };
                ViewGrouping::capture(doc, view, choice.clone(), allow, personal)
                    .map(Intent::Grouping)
            }
            Self::Sort(view) => {
                let Input::Sort(choice) = input else {
                    unreachable!()
                };
                ViewSort::capture(doc, view, choice.clone(), allow, personal).map(Intent::Sort)
            }
            Self::Filter(view) => {
                let Input::Filter(choice) = input else {
                    unreachable!()
                };
                ViewFilter::capture(doc, view, choice.clone(), allow, personal).map(Intent::Filter)
            }
            Self::Visibility { view, column } => {
                let Input::Visibility(value) = input else {
                    unreachable!()
                };
                ViewFieldVisibility::capture(doc, view, column, *value, allow, personal)
                    .map(Intent::Visibility)
            }
            Self::Precision(id) => {
                let Input::Precision(value) = input else {
                    unreachable!()
                };
                ColumnPrecision::capture(doc, id, *value, allow, personal).map(Intent::Precision)
            }
            Self::NewView(layout, grouping) => ViewCreate::capture_with_grouping(
                doc,
                name,
                *layout,
                grouping.as_deref(),
                allow,
                personal,
            )
            .map(Intent::Create),
            Self::Column(id) => {
                ColumnRename::capture(doc, id, name, allow, personal).map(Intent::Column)
            }
            Self::View(id) => ViewRename::capture(doc, id, name, allow, personal).map(Intent::View),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Input {
    Name(String),
    NewTextField(String),
    Precision(u8),
    Visibility(bool),
    Filter(ViewFilterChoice),
    Sort(ViewSortChoice),
    Grouping(ViewGroupingChoice),
}
impl Input {
    pub fn name(&self) -> Option<&str> {
        if let Self::Name(value) | Self::NewTextField(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
#[derive(Clone)]
pub(super) enum Intent {
    Column(ColumnRename),
    NewTextField(ColumnCreate),
    Precision(ColumnPrecision),
    View(ViewRename),
    Visibility(ViewFieldVisibility),
    Filter(ViewFilter),
    Sort(ViewSort),
    Grouping(ViewGrouping),
    Create(ViewCreate),
}
impl Intent {
    pub fn original(&self) -> &Document {
        match self {
            Self::Column(i) => i.original(),
            Self::NewTextField(i) => i.original(),
            Self::Precision(i) => i.original(),
            Self::View(i) => i.original(),
            Self::Visibility(i) => i.original(),
            Self::Filter(i) => i.original(),
            Self::Sort(i) => i.original(),
            Self::Grouping(i) => i.original(),
            Self::Create(i) => i.original(),
        }
    }
    pub fn input(&self) -> Input {
        match self {
            Self::Column(i) => Input::Name(i.desired_name().into()),
            Self::NewTextField(i) => Input::NewTextField(i.desired_name().into()),
            Self::View(i) => Input::Name(i.desired_name().into()),
            Self::Visibility(i) => Input::Visibility(i.desired_visible()),
            Self::Filter(i) => Input::Filter(i.desired_filter().clone()),
            Self::Sort(i) => Input::Sort(i.desired_sort().clone()),
            Self::Grouping(i) => Input::Grouping(i.desired_grouping().clone()),
            Self::Create(i) => Input::Name(i.desired_name().into()),
            Self::Precision(i) => Input::Precision(i.desired_precision()),
        }
    }
    pub fn observes_desired(&self, doc: &Document) -> bool {
        match self {
            Self::Column(i) => i.observes_desired(doc),
            Self::Precision(i) => i.observes_desired(doc),
            Self::View(i) => i.observes_desired(doc),
            Self::Visibility(i) => i.observes_desired(doc),
            Self::Filter(i) => i.observes_desired(doc),
            Self::Sort(i) => i.observes_desired(doc),
            Self::Grouping(i) => i.observes_desired(doc),
            Self::NewTextField(_) | Self::Create(_) => false, // A matching new view is never causal proof.
        }
    }
    pub fn admit(
        &self,
        doc: &Document,
        allow: Option<&str>,
        personal: &PersonalTaskLists,
    ) -> Option<Permit> {
        match self {
            Self::NewTextField(_) => None, // Population/current-user admission is mandatory.
            Self::Grouping(i) => ViewGroupingAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Grouping),
            Self::Sort(i) => ViewSortAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Sort),
            Self::Filter(i) => ViewFilterAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Filter),
            Self::Visibility(i) => ViewFieldVisibilityAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Visibility),
            Self::Precision(i) => ColumnPrecisionAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Precision),
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
    NewTextField(ColumnCreatePermit),
    Precision(ColumnPrecisionPermit),
    View(ViewRenamePermit),
    Visibility(ViewFieldVisibilityPermit),
    Filter(ViewFilterPermit),
    Sort(ViewSortPermit),
    Grouping(ViewGroupingPermit),
    Create(ViewCreatePermit),
}
impl Permit {
    pub fn is_text_creation(&self) -> bool {
        matches!(self, Self::NewTextField(_))
    }
    pub fn is_grouping(&self) -> bool {
        matches!(self, Self::Grouping(_))
    }
    pub fn is_sort(&self) -> bool {
        matches!(self, Self::Sort(_))
    }
    pub fn is_filter(&self) -> bool {
        matches!(self, Self::Filter(_))
    }
    pub fn is_visibility(&self) -> bool {
        matches!(self, Self::Visibility(_))
    }
    pub fn is_precision(&self) -> bool {
        matches!(self, Self::Precision(_))
    }
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
            Self::NewTextField(p) => {
                super_platinum_core::slack::api::list_column_create(client, session, p)
            }
            Self::Precision(p) => {
                super_platinum_core::slack::api::list_column_precision(client, session, p)
            }
            Self::Column(p) => {
                super_platinum_core::slack::api::list_column_rename(client, session, p)
            }
            Self::View(p) => super_platinum_core::slack::api::list_view_rename(client, session, p),
            Self::Sort(p) => super_platinum_core::slack::api::list_view_sort(client, session, p),
            Self::Grouping(p) => {
                super_platinum_core::slack::api::list_view_grouping(client, session, p)
            }
            Self::Filter(p) => {
                super_platinum_core::slack::api::list_view_filter(client, session, p)
            }
            Self::Visibility(p) => {
                super_platinum_core::slack::api::list_field_visibility(client, session, p)
            }
            Self::Create(p) => {
                super_platinum_core::slack::api::list_view_create(client, session, p)
            }
        }
    }
}
