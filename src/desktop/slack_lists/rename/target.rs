//! One typed registry/dispatch path for names and Number display precision.
use super::*;
use super_platinum_core::slack_lists::{
    BoardChoiceAction, BoardChoiceAttempt, BoardChoiceChange, BoardChoicePermit,
};
use super_platinum_core::slack_lists::{
    BoardGroupDelete, BoardGroupDeleteCellsPermit, BoardGroupDeleteSchemaPermit,
};
use super_platinum_core::slack_lists::{
    BoardLaneOrder, BoardLaneOrderAttempt, BoardLaneOrderDraft, BoardLaneOrderPermit,
};
use super_platinum_core::slack_lists::{BoardRecordMove, BoardRecordMovePermit};
use super_platinum_core::slack_lists::{ColumnCreate, ColumnCreatePermit};
use super_platinum_core::slack_lists::{ColumnDelete, ColumnDeletePermit};
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
use super_platinum_core::slack_lists::{
    ViewReorder, ViewReorderAttempt, ViewReorderDirection, ViewReorderPermit,
};
use super_platinum_core::slack_lists::{ViewSort, ViewSortAttempt, ViewSortChoice, ViewSortPermit};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Refinement(refinement_save::Action),
    Column(String),
    DeleteColumn(String),
    NewTextField(String),
    Precision(String),
    View(String),
    Reorder(String, ViewReorderDirection),
    LaneOrder(lane_order::Action),
    BoardChoice(board_choice::Choice),
    CardOrder(card_order::Action),
    GroupDelete(group_delete::Action),
    Filter(String),
    Sort(String),
    Grouping(String),
    Visibility { view: String, column: String },
    NewView(ViewLayout, Option<String>),
}
impl Target {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Refinement(a) => Some(a.view_id()),
            Self::NewTextField(v)
            | Self::Column(v)
            | Self::DeleteColumn(v)
            | Self::Precision(v)
            | Self::View(v)
            | Self::Sort(v)
            | Self::Grouping(v)
            | Self::Filter(v) => Some(v),
            Self::Reorder(v, _) => Some(v),
            Self::LaneOrder(a) => Some(&a.view),
            Self::BoardChoice(a) => Some(&a.view),
            Self::CardOrder(a) => Some(&a.drag.view),
            Self::GroupDelete(a) => Some(&a.view),
            Self::NewView(_, _) => None,
            Self::Visibility { view, .. } => Some(view),
        }
    }
    pub fn is_refinement(&self) -> bool {
        matches!(self, Self::Refinement(_))
    }
    pub fn is_reorder(&self) -> bool {
        matches!(self, Self::Reorder(..) | Self::LaneOrder(_))
    }
    pub fn is_group_delete(&self) -> bool {
        matches!(self, Self::GroupDelete(_))
    }
    pub fn is_card_order(&self) -> bool {
        matches!(self, Self::CardOrder(_))
    }
    pub fn is_board_choice(&self) -> bool {
        matches!(self, Self::BoardChoice(_))
    }
    pub fn is_lane_order(&self) -> bool {
        matches!(self, Self::LaneOrder(_))
    }
    pub fn is_deletion(&self) -> bool {
        matches!(self, Self::DeleteColumn(_))
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
            Self::Refinement(_)
                | Self::NewTextField(_)
                | Self::GroupDelete(_)
                | Self::CardOrder(_)
                | Self::BoardChoice(_)
                | Self::LaneOrder(_)
                | Self::Reorder(..)
                | Self::View(_)
                | Self::Sort(_)
                | Self::Grouping(_)
                | Self::Filter(_)
                | Self::NewView(_, _)
                | Self::Visibility { .. }
        )
    }
    pub fn noun(&self) -> &'static str {
        if self.is_group_delete() {
            return "group";
        }
        if self.is_card_order() {
            return "item";
        }
        if self.is_board_choice() {
            return "group";
        }
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
            Self::Refinement(a) => {
                if a.new {
                    Some("")
                } else {
                    doc.views.iter().find(|v| v["id"] == a.view_id())?["name"].as_str()
                }
            }
            Self::NewView(_, _) | Self::NewTextField(_) => Some(""),
            Self::Visibility { column, .. } => doc.column(column).map(|c| c.name.as_str()),
            Self::Column(id) | Self::DeleteColumn(id) | Self::Precision(id) => {
                doc.column(id).map(|c| c.name.as_str())
            }
            Self::CardOrder(a) => {
                doc.views.iter().find(|v| v["id"] == a.drag.view)?["name"].as_str()
            }
            Self::GroupDelete(a) => group_delete::name(doc, a),
            Self::BoardChoice(a) => board_choice::name(doc, a),
            Self::LaneOrder(a) => doc
                .views
                .iter()
                .find(|v| v["id"] == a.view)?
                .get("name")?
                .as_str(),
            Self::View(id)
            | Self::Reorder(id, _)
            | Self::Sort(id)
            | Self::Filter(id)
            | Self::Grouping(id) => doc
                .views
                .iter()
                .find(|v| v["id"] == id.as_str())?
                .get("name")?
                .as_str(),
        }
    }
    pub fn supports(&self, doc: &Document) -> bool {
        match self {
            Self::Refinement(a) => a.supports(doc),
            Self::NewTextField(id) => doc
                .views
                .iter()
                .any(|v| v["id"] == id.as_str() && v["type"] == "table" && ViewRename::supports(v)),
            Self::Reorder(id, direction) => ViewReorder::choices(doc, id).contains(direction),
            Self::CardOrder(a) => card_order::supports(doc, a),
            Self::GroupDelete(a) => group_delete::supports(doc, a),
            Self::BoardChoice(a) => board_choice::supports(doc, a),
            Self::LaneOrder(a) => {
                BoardLaneOrder::refined_order(doc, &a.refinements, a.draft.as_ref()).is_ok_and(
                    |ids| {
                        a.source != a.target && ids.contains(&a.source) && ids.contains(&a.target)
                    },
                )
            }
            Self::NewView(_, _) => ViewCreate::supports(doc),
            Self::Filter(view) => ViewFilter::current(doc, view).is_some(),
            Self::Sort(view) => ViewSort::current(doc, view).is_some(),
            Self::Grouping(view) => ViewGrouping::current(doc, view).is_some(),
            Self::Visibility { view, column } => {
                ViewFieldVisibility::current(doc, view, column).is_some()
            }
            Self::Precision(id) => precision::current(doc, id).is_some(),
            Self::DeleteColumn(id) => ColumnDelete::supports(doc, id),
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
        if self.is_group_delete() != matches!(input, Input::GroupDelete)
            || self.is_card_order() != matches!(input, Input::CardOrder)
            || self.is_board_choice() != matches!(input, Input::BoardChoice(_))
            || self.is_reorder() != matches!(input, Input::Reorder)
            || self.is_deletion() != matches!(input, Input::DeleteColumn)
            || self.is_text_creation() != matches!(input, Input::NewTextField(_))
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
            Self::Refinement(a) => a.intent(doc, name, allow, personal).map(Intent::Refinement),
            Self::GroupDelete(_) => Err(super_platinum_core::slack_lists::ListError(
                "Group deletion requires its captured population and two-effect admission.",
            )),
            Self::CardOrder(_) => Err(super_platinum_core::slack_lists::ListError(
                "Card moves require the captured population and projection.",
            )),
            Self::BoardChoice(a) => {
                let Input::BoardChoice(action) = input else {
                    unreachable!()
                };
                if !a.accepts(action) {
                    return Err(super_platinum_core::slack_lists::ListError(
                        "The group action changed.",
                    ));
                }
                BoardChoiceChange::capture_refined(
                    doc,
                    &a.refinements,
                    action.clone(),
                    allow,
                    personal,
                )
                .map(Intent::BoardChoice)
            }
            Self::LaneOrder(a) => BoardLaneOrder::capture_refined(
                doc,
                &a.refinements,
                a.draft.as_ref(),
                &a.source,
                &a.target,
                a.placement,
                allow,
                personal,
            )
            .map(Intent::LaneOrder),
            Self::Reorder(id, direction) => {
                ViewReorder::capture(doc, id, *direction, allow, personal).map(Intent::Reorder)
            }
            Self::DeleteColumn(_) => Err(super_platinum_core::slack_lists::ListError(
                "Field deletion requires authenticated schema permissions.",
            )),
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
    GroupDelete,
    CardOrder,
    BoardChoice(BoardChoiceAction),
    Reorder,
    DeleteColumn,
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
    Refinement(super_platinum_core::slack_lists::ViewRefinementSave),
    Column(ColumnRename),
    DeleteColumn(ColumnDelete),
    NewTextField(ColumnCreate),
    Precision(ColumnPrecision),
    View(ViewRename),
    Reorder(ViewReorder),
    LaneOrder(BoardLaneOrder),
    BoardChoice(BoardChoiceChange),
    CardOrder(BoardRecordMove),
    GroupDelete(BoardGroupDelete),
    Visibility(ViewFieldVisibility),
    Filter(ViewFilter),
    Sort(ViewSort),
    Grouping(ViewGrouping),
    Create(ViewCreate),
}
impl Intent {
    pub fn original(&self) -> &Document {
        match self {
            Self::Refinement(i) => i.original(),
            Self::Column(i) => i.original(),
            Self::DeleteColumn(i) => i.original(),
            Self::NewTextField(i) => i.original(),
            Self::Precision(i) => i.original(),
            Self::View(i) => i.original(),
            Self::Reorder(i) => i.original(),
            Self::LaneOrder(i) => i.original(),
            Self::BoardChoice(i) => i.original(),
            Self::CardOrder(i) => i.original(),
            Self::GroupDelete(i) => i.original(),
            Self::Visibility(i) => i.original(),
            Self::Filter(i) => i.original(),
            Self::Sort(i) => i.original(),
            Self::Grouping(i) => i.original(),
            Self::Create(i) => i.original(),
        }
    }
    pub fn input(&self) -> Input {
        match self {
            Self::Refinement(i) => Input::Name(match i.destination() {
                super_platinum_core::slack_lists::ViewRefinementDestination::Existing => i
                    .original()
                    .views
                    .iter()
                    .find(|v| v["id"] == i.view_id())
                    .and_then(|v| v["name"].as_str())
                    .unwrap_or_default()
                    .into(),
                super_platinum_core::slack_lists::ViewRefinementDestination::New { name } => {
                    name.clone()
                }
            }),
            Self::Reorder(_) | Self::LaneOrder(_) => Input::Reorder,
            Self::BoardChoice(i) => Input::BoardChoice(i.action().clone()),
            Self::CardOrder(_) => Input::CardOrder,
            Self::GroupDelete(_) => Input::GroupDelete,
            Self::DeleteColumn(_) => Input::DeleteColumn,
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
            Self::Refinement(i) => {
                matches!(
                    i.destination(),
                    super_platinum_core::slack_lists::ViewRefinementDestination::Existing
                ) && i.observed_view_id(doc).is_some()
            }
            Self::Column(i) => i.observes_desired(doc),
            Self::DeleteColumn(i) => i.observes_desired(doc),
            Self::Precision(i) => i.observes_desired(doc),
            Self::View(i) => i.observes_desired(doc),
            Self::Reorder(i) => i.observes_desired(doc),
            Self::LaneOrder(i) => i.observes_desired(doc),
            Self::BoardChoice(i) => i.observes_desired(doc),
            Self::GroupDelete(_) => false,
            Self::CardOrder(_) => false, // Fresh exact population is mandatory.
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
            Self::Refinement(_) => None, // Original worker retains the one-use ACK handoff.
            Self::GroupDelete(_) => None,
            Self::CardOrder(_) => None, // Fresh population/projection admission is mandatory.
            Self::BoardChoice(i) => BoardChoiceAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::BoardChoice),
            Self::LaneOrder(i) => BoardLaneOrderAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::LaneOrder),
            Self::Reorder(i) => ViewReorderAttempt::new(i.clone())
                .admit(doc, allow, Some(personal))
                .ok()
                .map(Permit::Reorder),
            Self::DeleteColumn(_) => None, // Authenticated-user admission is mandatory.
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
    Refinement(super_platinum_core::slack_lists::ViewRefinementSavePermit),
    Column(ColumnRenamePermit),
    DeleteColumn(ColumnDeletePermit),
    NewTextField(ColumnCreatePermit),
    Precision(ColumnPrecisionPermit),
    View(ViewRenamePermit),
    Reorder(ViewReorderPermit),
    LaneOrder(BoardLaneOrderPermit),
    BoardChoice(BoardChoicePermit),
    CardOrder(BoardRecordMovePermit),
    GroupDeleteSchema(BoardGroupDeleteSchemaPermit),
    GroupDeleteCells(BoardGroupDeleteCellsPermit),
    Visibility(ViewFieldVisibilityPermit),
    Filter(ViewFilterPermit),
    Sort(ViewSortPermit),
    Grouping(ViewGroupingPermit),
    Create(ViewCreatePermit),
}
impl Permit {
    pub fn is_card_order(&self) -> bool {
        matches!(self, Self::CardOrder(_))
    }
    pub fn is_board_choice(&self) -> bool {
        matches!(self, Self::BoardChoice(_))
    }
    pub fn is_lane_order(&self) -> bool {
        matches!(self, Self::LaneOrder(_))
    }
    pub fn is_reorder(&self) -> bool {
        matches!(self, Self::Reorder(..))
    }
    pub fn is_deletion(&self) -> bool {
        matches!(self, Self::DeleteColumn(_))
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
            Self::Refinement(p) => {
                super_platinum_core::slack::api::list_view_refinement_save(client, session, p)
            }
            Self::DeleteColumn(p) => {
                super_platinum_core::slack::api::list_column_delete(client, session, p)
            }
            Self::NewTextField(p) => {
                super_platinum_core::slack::api::list_column_create(client, session, p)
            }
            Self::Precision(p) => {
                super_platinum_core::slack::api::list_column_precision(client, session, p)
            }
            Self::Column(p) => {
                super_platinum_core::slack::api::list_column_rename(client, session, p)
            }
            Self::CardOrder(p) => {
                super_platinum_core::slack::api::list_board_record_move(client, session, p)
            }
            Self::GroupDeleteSchema(p) => {
                super_platinum_core::slack::api::list_board_group_delete_schema(client, session, p)
            }
            Self::GroupDeleteCells(p) => {
                super_platinum_core::slack::api::list_board_group_delete_cells(client, session, p)
            }
            Self::BoardChoice(p) => {
                super_platinum_core::slack::api::list_board_choice(client, session, p)
            }
            Self::LaneOrder(p) => {
                super_platinum_core::slack::api::list_board_lane_order(client, session, p)
            }
            Self::Reorder(p) => {
                super_platinum_core::slack::api::list_view_reorder(client, session, p)
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
