//! Embedded first publication. One original modal owner, separate operation receipts.
use super::*;
mod actions;
mod confirmation_add;
mod confirmation_share;
pub(crate) use confirmation_share::delivery_row as share_delivery_row;
mod execute;
mod native;
mod readback;
mod receipt;
mod validation;
mod view;
pub(in crate::app_home::about::management) use actions::{cancel, open, submit};
pub(in crate::app_home::about::management) use readback::{check, close, resume, review_available};
pub(crate) use receipt::snapshot;
pub(in crate::app_home::about::management) use view::panel;

pub(super) struct Local {
    title: String,
    description: String,
    icon_url: String,
    more: bool,
    defaults: bool,
    epoch: u64,
    notice: Option<String>,
    avatar: avatar::State,
    share: bool,
}
mod avatar;
mod cropper;
impl Local {
    fn new(o: &Original) -> Self {
        let w = o.editor.raw_workflow();
        Self {
            title: clean(w["title"].as_str().unwrap_or_default()),
            description: clean(w["description"].as_str().unwrap_or_default()),
            icon_url: w["icons"]["image_192"].as_str().unwrap_or_default().into(),
            more: false,
            defaults: w["collaborators"] == json!([o.owner.ctx.session.user_id])
                && o.editor
                    .run_permissions()
                    .is_some_and(|v| v["permission_type"] == "everyone")
                && o.editor.copy_permissions()["permission_type"] == "collaborators",
            epoch: next(),
            notice: None,
            avatar: avatar::State::default(),
            share: false,
        }
    }
    fn expected(&self) -> Value {
        json!({"epoch":self.epoch,"title":self.title,"description":self.description,
            "icon":self.icon_url,"more":self.more,"pane":if self.share {"share"} else {self.avatar.pane.name()},"avatarPending":self.avatar.pending,"crop":self.avatar.upload.as_ref().map(|_|json!({"x":self.avatar.x,"y":self.avatar.y,"size":self.avatar.size})),"cropper":self.avatar.cropper.geometry})
    }
    fn valid(&self) -> bool {
        self.avatar.pane == avatar::Pane::Details
            && self.avatar.pane_bound
            && protocol::validate_name(&self.title).is_ok()
            && !protocol::description_invalid(&self.description, 80)
            && !self.avatar.pending
            && self.avatar.upload.is_none()
    }
}
fn clean(value: &str) -> String {
    value.replace(['(', ')'], "")
}

// Action-only stamp: background settings reads retain their existing owner.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailsAction {
    Standalone,
    Embedded(u64),
}
impl DetailsAction {
    fn capture(embedded: bool, local: Option<&Local>) -> Option<Self> {
        if !embedded {
            return Some(Self::Standalone);
        }
        local
            .filter(|l| l.avatar.pane == avatar::Pane::Details && l.avatar.pane_bound)
            .map(|l| Self::Embedded(l.epoch))
    }
    pub(super) fn current(self, c: &Confirmation) -> bool {
        Self::capture(c.original.embedded, c.finish_up.as_ref()) == Some(self)
    }
}
pub(super) fn details_action(c: &Confirmation) -> Option<DetailsAction> {
    DetailsAction::capture(c.original.embedded, c.finish_up.as_ref())
}

pub(in crate::app_home::about::management) struct Attempt {
    original: Original,
    lease: read::Lease,
    local: Local,
    settings: Option<settings::Settings>,
    managers: Option<managers::Managers>,
    metadata: Option<MutationPlan>,
    metadata_replies: Vec<Option<Reply>>,
    acknowledged: Vec<(protocol::ReadKind, Value)>,
    copy: Option<MutationPlan>,
    copy_reply: Option<Reply>,
    run: Option<groups::Run>,
    manager: Option<managers::Group>,
    publication: MutationPlan,
    publication_reply: Option<Reply>,
    followup: Option<protocol::FollowUpPlan>,
    follow_reply: Option<Reply>,
    validation: Value,
    phase: &'static str,
    continuation: Option<&'static str>,
    finished: bool,
    complete: bool,
    resolved: bool,
    closed: bool,
    dialog_original: Option<Original>,
    readback: Option<(Editor, bool)>,
    release: Option<Release>,
    confirmation_add: Option<confirmation_add::State>,
    confirmation_share: Option<confirmation_share::State>,
}
pub(in crate::app_home::about::management) fn shown(s: &ShellState, visit: u64) -> bool {
    document(s, visit).is_some_and(|d| supported_first(&d.editor))
}
fn supported_first(editor: &Editor) -> bool {
    let w = editor.raw_workflow();
    w["is_published"] == false
        && match w.get("last_published_version_id") {
            None | Some(Value::Null) => true,
            Some(Value::String(v)) => v.is_empty(),
            _ => false,
        }
        && editor.workflow_id().starts_with("Wf")
        && editor.trigger().is_some_and(|t| t["type"] == "shortcut")
        && super::supported(editor)
}
pub(in crate::app_home::about::management) fn active(s: &ShellState, visit: u64) -> bool {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_ref()
        .is_some_and(|c| c.original.visit == visit && c.original.embedded)
        || document(s, visit)
            .and_then(|d| d.pending)
            .is_some_and(|id| attempt(s, id).is_some_and(|a| !a.closed))
}
fn local(s: &ShellState, visit: u64, id: u64) -> Option<(&Confirmation, &Local)> {
    let c = settings::owned(s, visit, id)?;
    Some((c, c.finish_up.as_ref()?))
}
fn attempt(s: &ShellState, id: u64) -> Option<&Attempt> {
    s.app_home
        .about
        .management
        .permissions
        .first_attempts
        .iter()
        .find(|a| a.original.id == id)
}
fn attempt_mut(s: &mut ShellState, id: u64) -> Option<&mut Attempt> {
    s.app_home
        .about
        .management
        .permissions
        .first_attempts
        .iter_mut()
        .find(|a| a.original.id == id)
}
fn exact(s: &ShellState, id: u64) -> bool {
    attempt(s, id).is_some_and(|a| {
        !a.finished
            && a.lease.current(s)
            && a.original.current(s, Some(id))
            && a.settings
                .as_ref()
                .is_none_or(|v| v.source_current(s, &a.original))
            && a.managers
                .as_ref()
                .is_none_or(|m| m.current(s, &a.original))
    })
}
// Rebuild only from original acquired envelopes and actual completed ACKs.
// Arrival order is the controller's source publication order; proposed fields
// never enter this working editor and Original remains the custody baseline.
fn working_editor(a: &Attempt) -> Result<Editor, Error> {
    fold_acknowledged(&a.original.editor, &a.acknowledged)
}
fn fold_acknowledged(
    original: &Editor,
    acks: &[(protocol::ReadKind, Value)],
) -> Result<Editor, Error> {
    let mut responses = original.responses().clone();
    for (kind, raw) in acks {
        responses.insert(*kind, raw.clone());
    }
    let mut load = EditorLoad::new(original.workflow_id())?;
    while let Some(request) = load.requests().into_iter().next() {
        let raw = responses
            .get(&request.kind)
            .cloned()
            .ok_or_else(unavailable)?;
        load.accept(request.kind, raw)?;
    }
    load.editor()
}
fn workflow_ack(o: &Original, raw: &Value) -> bool {
    raw["decorated_workflow"]["id"] == o.editor.raw_workflow()["id"]
        && raw["decorated_workflow"]["app_id"] == o.editor.raw_workflow()["app_id"]
        && raw["decorated_workflow"]["steps"].is_array()
}
fn trigger_ack(o: &Original, raw: &Value) -> bool {
    o.editor.trigger().is_some_and(|t| {
        raw["trigger"]["id"] == t["id"]
            && raw["trigger"]["workflow"]["workflow_id"] == json!(o.editor.workflow_id())
            && raw["trigger"]["workflow"]["app_id"] == o.editor.raw_workflow()["app_id"]
    })
}
async fn authorized(state: Signal<ShellState>, id: u64) -> bool {
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|_| exact(&s, id))
            .map(|a| (a.original.clone(), a.local.expected()))
    };
    let Some((o, e)) = captured else { return false };
    native::current(&o, &e, None).await && exact(&state.peek(), id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_action_stamp_retires_across_panes_without_changing_standalone_admission() {
        let (s, _, id) = super::super::tests::prepared();
        let o = &super::super::run::attempt(&s, id).unwrap().original;
        let mut local = Local::new(o);
        assert!(DetailsAction::capture(true, Some(&local)).is_none());
        local.avatar.pane_bound = true;
        let before = DetailsAction::capture(true, Some(&local)).unwrap();
        local.avatar.pane = avatar::Pane::Upload;
        local.avatar.pending = true;
        assert!(DetailsAction::capture(true, Some(&local)).is_none());
        local.avatar.pane = avatar::Pane::Details;
        local.avatar.pending = false;
        local.avatar.pane_bound = false;
        local.epoch = next();
        assert!(DetailsAction::capture(true, Some(&local)).is_none());
        local.avatar.pane_bound = true;
        assert!(DetailsAction::capture(true, Some(&local)) != Some(before));
        assert!(DetailsAction::capture(false, Some(&local)) == Some(DetailsAction::Standalone));
        assert!(DetailsAction::capture(false, None) == Some(DetailsAction::Standalone));
    }
    #[test]
    fn metadata_initialization_removes_only_parentheses() {
        assert_eq!(clean(" (A) & <b> \u{4e2d} "), " A & <b> \u{4e2d} ");
    }
    #[test]
    fn actual_trigger_metadata_ack_is_the_publication_baseline_without_changing_original() {
        let (s, _, id) = super::super::tests::prepared();
        let o = &super::super::run::attempt(&s, id).unwrap().original;
        let mut raw = o.editor.responses()[&protocol::ReadKind::TriggerInfo].clone();
        raw["trigger"]["name"] = json!("Actually acknowledged name");
        raw["trigger"]["description"] = json!("Actually acknowledged description");
        let before = o.editor.trigger().cloned();
        let working =
            fold_acknowledged(&o.editor, &[(protocol::ReadKind::TriggerInfo, raw.clone())])
                .unwrap();
        let p = MutationPlan::new(&working, protocol::Command::Published(true)).unwrap();
        assert_eq!(p.original().trigger(), Some(&raw["trigger"]));
        assert_eq!(o.editor.trigger(), before.as_ref());
        raw["trigger"]["workflow"]["workflow_id"] = json!("WfDifferent");
        assert!(fold_acknowledged(&o.editor, &[(protocol::ReadKind::TriggerInfo, raw)]).is_err());
    }
    #[test]
    fn working_workflow_uses_real_arrival_order_and_does_not_substitute_proposed_fields() {
        let (s, _, id) = super::super::tests::prepared();
        let o = &super::super::run::attempt(&s, id).unwrap().original;
        let mut first = o.editor.latest_response().clone();
        first["decorated_workflow"]["title"] = json!("Metadata ACK");
        let mut later = first.clone();
        later["decorated_workflow"]["collaborators"] = json!(["UACTUAL01"]);
        let working = fold_acknowledged(
            &o.editor,
            &[
                (protocol::ReadKind::Latest, first),
                (protocol::ReadKind::Latest, later.clone()),
            ],
        )
        .unwrap();
        assert_eq!(working.raw_workflow(), &later["decorated_workflow"]);
        assert_eq!(fold_acknowledged(&o.editor, &[]).unwrap(), o.editor);
    }
    #[test]
    fn local_validation_uses_source_name_and_required_utf16_description() {
        let (s, _, id) = super::super::tests::prepared();
        let o = &super::super::run::attempt(&s, id).unwrap().original;
        let mut local = Local::new(o);
        local.avatar.pane_bound = true;
        local.title = "Workflow title".into();
        local.description = "x".repeat(80);
        assert!(local.valid());
        local.description.push('x');
        assert!(!local.valid());
        local.description = "\u{a0}".into();
        assert!(!local.valid());
        local.description = "Valid".into();
        local.title = "constructor".into();
        assert!(!local.valid());
        local.title = "Bad/name".into();
        assert!(!local.valid());
    }
}

pub(super) fn retained(book: &Book, id: u64) -> bool {
    book.first_attempts.iter().any(|a| a.original.id == id)
}

fn bind_visible(state: Signal<ShellState>, visit: u64, id: u64) {
    if local(&state.peek(), visit, id).is_some() {
        actions::bind(state, visit, id);
        return;
    }
    let captured = {
        let s = state.peek();
        attempt(&s, id)
            .filter(|a| !a.closed && a.original.visit == visit)
            .map(|a| {
                (
                    a.dialog_original.as_ref().unwrap_or(&a.original).clone(),
                    a.local.expected(),
                )
            })
    };
    if let Some((o, e)) = captured {
        dioxus::core::spawn_forever(async move {
            native::bind(&o, &e).await;
        });
    }
}
