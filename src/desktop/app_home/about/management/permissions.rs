//! Owned copy-access settings; metadata buffers and independent receipts remain separate.
use super::*;
use crate::app_home::inputs::native::Release;
mod finish;
pub(crate) mod finish_up;
mod groups;
pub(super) mod managers;
mod native;
mod readback;
mod receipt;
mod run;
pub(super) mod settings;
pub(super) use readback::check;
pub(super) use receipt::outcomes;
pub(crate) use receipt::snapshot;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Book {
    confirmation: Option<Confirmation>,
    pub(super) attempts: Vec<Attempt>,
    pub(super) first_attempts: Vec<finish_up::Attempt>,
    pub(super) first_avatar_receipts: Vec<Value>,
}
impl Book {
    pub(super) fn retire_confirmation(&mut self) {
        if let Some(c) = self.confirmation.take() {
            c.release.release();
        }
    }
}
#[derive(Clone)]
struct Original {
    embedded: bool,
    id: u64,
    visit: u64,
    document: u64,
    revision: u64,
    title: String,
    description: String,
    editor: Editor,
    policy: policy::Snapshot,
    owner: Owner,
}
struct Confirmation {
    finish_up: Option<finish_up::Local>,
    original: Original,
    release: Release,
    selected: String,
    settings: Option<settings::Settings>,
    managers: Option<managers::Managers>,
}
pub(super) struct Attempt {
    original: Original,
    lease: read::Lease,
    plan: MutationPlan,
    copy_changed: bool,
    run_group: Option<groups::Run>,
    manager_group: Option<managers::Group>,
    manager_source: Option<managers::Managers>,
    departure: managers::departure::Departure,
    settings_source: Option<settings::Settings>,
    replies: Vec<Option<Reply>>,
    finished: bool,
    complete: bool,
    resolved: bool,
    desired: String,
    readback: Option<(Editor, bool)>,
    no_send: bool,
    release: Option<Release>,
}
enum Reply {
    Raw(Value),
    Failure(&'static str),
}
impl Original {
    fn capture(s: &ShellState, visit: u64) -> Option<Self> {
        if !available(s, visit) {
            return None;
        }
        let d = document(s, visit)?;
        let h = s.app_home.about.management.surface.as_ref()?;
        Some(Self {
            embedded: false,
            id: next(),
            visit,
            document: d.id,
            revision: d.revision,
            title: d.title.clone(),
            description: d.description.clone(),
            editor: d.editor.clone(),
            policy: h.policy.clone()?,
            owner: h.owner.clone(),
        })
    }
    fn current(&self, s: &ShellState, pending: Option<u64>) -> bool {
        admitted(s, self.visit)
            && self.owner.current(s)
            && self.policy.current(s)
            && self.policy.can_create()
            && s.app_home
                .about
                .management
                .surface
                .as_ref()
                .is_some_and(|h| h.policy.as_ref().is_some_and(|p| p.same(&self.policy)))
            && document(s, self.visit).is_some_and(|d| {
                d.id == self.document
                    && d.revision == self.revision
                    && d.title == self.title
                    && d.description == self.description
                    && d.pending == pending
                    && !d.activity.active
                    && self.editor.same_revision(&d.editor)
                    && allowed_editor(&d.editor, &self.owner.ctx.session.user_id)
            })
    }
    fn expected(&self) -> Value {
        {
            let mut value = json!({"id":self.id,"visit":self.visit,"document":self.document,"revision":self.revision,
            "title":self.title,"description":self.description});
            if self.embedded {
                value["finishUp"] = json!(true);
            }
            value
        }
    }
}
fn supported(editor: &Editor) -> bool {
    matches!(
        editor.copy_permissions()["permission_type"].as_str(),
        Some("everyone" | "collaborators")
    ) && valid_unchanged_run(editor.run_permissions())
}
// The source's local error checks use concatenated selected IDs. Team/org
// partition and target-policy drift require producers outside EditorLoad.
fn valid_unchanged_run(run: Option<&Value>) -> bool {
    let Some(run) = run else { return true };
    let Some(mode) = run["permission_type"]
        .as_str()
        .filter(|v| matches!(*v, "everyone" | "app_collaborators" | "named_entities"))
    else {
        return false;
    };
    let mut count = 0;
    for key in ["user_ids", "channel_ids", "team_ids", "org_ids"] {
        let Some(value) = run.get(key) else { continue };
        let Some(ids) = value.as_array() else {
            return false;
        };
        if ids.iter().any(|id| id.as_str().is_none_or(str::is_empty)) {
            return false;
        }
        count += ids.len();
    }
    count <= 100 && (mode != "named_entities" || count > 0)
}
pub(super) fn available(s: &ShellState, visit: u64) -> bool {
    editable(s, visit) && document(s, visit).is_some_and(|d| supported(&d.editor))
}
fn close_owned(
    mut state: Signal<ShellState>,
    visit: u64,
    id: u64,
    original: &Original,
    selected: &str,
    settings_epoch: Option<u64>,
) {
    let mut s = state.write();
    if !original.current(&s, None)
        || confirmation(&s, visit) != Some(id)
        || !s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_ref()
            .is_some_and(|c| {
                c.selected == selected && c.settings.as_ref().map(|v| v.epoch) == settings_epoch
            })
    {
        return;
    }
    let c = s
        .app_home
        .about
        .management
        .permissions
        .confirmation
        .take()
        .unwrap();
    document_mut(&mut s, visit).unwrap().revision = next();
    c.release.release();
}

pub(super) fn select(mut state: Signal<ShellState>, visit: u64, id: u64, value: String) {
    if !matches!(value.as_str(), "everyone" | "collaborators") {
        return;
    }
    let captured = {
        let s = state.peek();
        settings::owned(&s, visit, id).and_then(|c| {
            let pane = finish_up::details_action(c)?;
            Some((
                c.original.clone(),
                c.settings.as_ref().map(|v| v.expected()),
                pane,
            ))
        })
    };
    let Some((original, expected, pane)) = captured else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !managers::verify(state, &original).await {
            return;
        }
        if let Some(e) = &expected {
            if !native::settings(&original, e, Some(&json!({"kind":"copy","value":value}))).await {
                return;
            }
        }
        if !native::choice(&original, &value).await {
            return;
        }
        let mut s = state.write();
        let Some(c) = settings::owned(&s, visit, id) else {
            return;
        };
        if !pane.current(c) {
            return;
        }
        if c.settings.as_ref().map(|v| v.epoch)
            != expected.as_ref().and_then(|e| e["epoch"].as_u64())
        {
            return;
        }
        let c = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_mut()
            .unwrap();
        c.selected = value.clone();
        if let Some(v) = c.settings.as_mut() {
            v.copy = value;
            v.epoch = next();
        }
        drop(s);
        settings::bind_settings(state, visit, id);
    });
}
pub(super) fn bind(state: Signal<ShellState>, visit: u64, id: u64) {
    let original = {
        let s = state.peek();
        s.app_home
            .about
            .management
            .permissions
            .confirmation
            .as_ref()
            .filter(|c| c.original.id == id && c.original.visit == visit)
            .map(|c| c.original.clone())
    };
    if let Some(original) = original {
        dioxus::core::spawn_forever(async move {
            native::bind(&original).await;
            let expected = {
                let s = state.peek();
                settings::owned(&s, visit, id)
                    .and_then(|c| c.settings.as_ref().map(|v| v.expected()))
            };
            if let Some(expected) = expected {
                if native::settings(&original, &expected, None).await {
                    settings::hydrate(state, visit, id, false);
                }
            }
        });
    }
}
pub(super) fn selected(s: &ShellState, visit: u64) -> Option<String> {
    confirmation(s, visit)?;
    Some(
        s.app_home
            .about
            .management
            .permissions
            .confirmation
            .as_ref()?
            .selected
            .clone(),
    )
}
pub(super) fn save_enabled(s: &ShellState, visit: u64) -> bool {
    let Some(id) = confirmation(s, visit) else {
        return false;
    };
    settings::owned(s, visit, id).is_some_and(|c| {
        c.managers
            .as_ref()
            .is_some_and(|m| m.current(s, &c.original) && m.valid() && !m.speed_bump)
            && c.settings
                .as_ref()
                .is_none_or(|v| v.current(s, &c.original) && v.desired().is_ok())
    })
}
pub(super) fn confirming(s: &ShellState, visit: u64) -> bool {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_ref()
        .is_some_and(|c| c.original.visit == visit)
}
pub(super) fn confirmation(s: &ShellState, visit: u64) -> Option<u64> {
    s.app_home
        .about
        .management
        .permissions
        .confirmation
        .as_ref()
        .filter(|c| c.original.visit == visit && c.original.current(s, None))
        .map(|c| c.original.id)
}
pub(super) fn retained(book: &Book, pending: Option<u64>) -> bool {
    pending.is_some_and(|id| {
        book.attempts.iter().any(|a| a.original.id == id) || finish_up::retained(book, id)
    })
}
pub(super) fn owns(s: &ShellState, visit: u64) -> bool {
    retained(
        &s.app_home.about.management.permissions,
        document(s, visit).and_then(|d| d.pending),
    )
}
pub(super) fn open(mut state: Signal<ShellState>, visit: u64) {
    let Some(original) = Original::capture(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let Some(release) = native::lock(&original).await else {
            return;
        };
        let mut s = state.write();
        if !original.current(&s, None) || !editable(&s, visit) {
            release.release();
            return;
        }
        s.app_home
            .about
            .management
            .permissions
            .retire_confirmation();
        let id = original.id;
        let settings = settings::Settings::capture(&s, &original);
        let managers = managers::Managers::capture(&s, &original);
        s.app_home.about.management.permissions.confirmation = Some(Confirmation {
            finish_up: None,
            selected: original.editor.copy_permissions()["permission_type"]
                .as_str()
                .unwrap()
                .into(),
            original,
            release,
            settings,
            managers,
        });
        drop(s);
        // Only an open confirmation has this bounded ownership watcher. It
        // cannot keep a replaced view locked or authorize another request.
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let retired = {
                let s = state.peek();
                let Some(c) = s
                    .app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_ref()
                    .filter(|c| c.original.id == id)
                else {
                    break;
                };
                !c.original.current(&s, None)
            };
            if retired {
                let mut s = state.write();
                if s.app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_ref()
                    .is_some_and(|c| c.original.id == id)
                {
                    s.app_home
                        .about
                        .management
                        .permissions
                        .retire_confirmation();
                }
                break;
            }
        }
    });
}
pub(super) fn cancel(mut state: Signal<ShellState>, visit: u64, id: u64) {
    if finish_up::active(&state.peek(), visit) {
        finish_up::cancel(state, visit, id);
        return;
    }
    let (original, selected, expected) = {
        let s = state.peek();
        let Some(c) = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_ref()
            .filter(|c| c.original.visit == visit && c.original.id == id)
        else {
            return;
        };
        (
            c.original.clone(),
            c.selected.clone(),
            c.settings.as_ref().map(|v| v.expected()),
        )
    };
    dioxus::core::spawn_forever(async move {
        if !managers::verify(state, &original).await {
            return;
        }
        if let Some(expected) = &expected {
            if !native::settings(&original, expected, None).await {
                return;
            }
        }
        // A failed witness must never cause a controlled-value render over a
        // native buffer that has not reached serial state.
        if !native::choice(&original, &selected).await {
            return;
        }
        close_owned(
            state,
            visit,
            id,
            &original,
            &selected,
            expected.as_ref().and_then(|v| v["epoch"].as_u64()),
        );
    });
}
pub(super) fn confirm(mut state: Signal<ShellState>, visit: u64, id: u64) {
    if finish_up::active(&state.peek(), visit) {
        return;
    }
    let (original, desired, settings_expected, run_command, settings_epoch) = {
        let s = state.peek();
        let Some(c) = s
            .app_home
            .about
            .management
            .permissions
            .confirmation
            .as_ref()
            .filter(|c| {
                c.original.id == id && c.original.visit == visit && c.original.current(&s, None)
            })
        else {
            return;
        };
        let run_command = match c.settings.as_ref().map(|v| v.desired()).transpose() {
            Ok(command) => command.flatten(),
            Err(_) => return,
        };
        if c.settings
            .as_ref()
            .is_some_and(|v| !v.current(&s, &c.original))
        {
            return;
        }
        (
            c.original.clone(),
            c.selected.clone(),
            c.settings.as_ref().map(|v| v.expected()),
            run_command,
            c.settings.as_ref().map(|v| v.epoch),
        )
    };
    dioxus::core::spawn_forever(async move {
        if !managers::verify(state, &original).await {
            return;
        }
        if let Some(expected) = &settings_expected {
            if !native::settings(&original, expected, None).await {
                return;
            }
        }
        if !native::choice(&original, &desired).await {
            return;
        }
        let manager_plan = {
            let mut s = state.write();
            let Some(c) = settings::owned(&s, visit, id) else {
                return;
            };
            let Some(m) = c
                .managers
                .as_ref()
                .filter(|m| m.current(&s, &original) && m.valid())
            else {
                return;
            };
            if m.draft.self_removal() && !m.speed_bump {
                let m = s
                    .app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_mut()
                    .unwrap()
                    .managers
                    .as_mut()
                    .unwrap();
                m.speed_bump = true;
                m.epoch = next();
                drop(s);
                managers::bind(state, visit, id);
                return;
            }
            let Ok(plan) = m.draft.plan() else { return };
            plan
        };
        let manager_changed = !manager_plan.slots().is_empty();
        let copy_changed = original.editor.copy_permissions()["permission_type"] != desired;
        if !copy_changed && run_command.is_none() && !manager_changed {
            close_owned(state, visit, id, &original, &desired, settings_epoch);
            return;
        }
        let admitted = {
            let mut s = state.write();
            if !original.current(&s, None)
                || s.app_home.about.management.permissions.attempts.len() >= 128
                || confirmation(&s, visit) != Some(id)
                || s.app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_ref()
                    .and_then(|c| c.settings.as_ref().map(|v| v.epoch))
                    != settings_epoch
                || s.app_home
                    .about
                    .management
                    .permissions
                    .confirmation
                    .as_ref()
                    .is_some_and(|c| {
                        c.settings
                            .as_ref()
                            .is_some_and(|v| !v.current(&s, &original))
                    })
            {
                return;
            }
            let run_group = match run_command
                .map(|command| groups::Run::new(&original.editor, command))
                .transpose()
            {
                Ok(v) => v,
                Err(_) => return,
            };
            let Ok(plan) = MutationPlan::new(
                &original.editor,
                protocol::Command::CopyPermission(desired.clone()),
            ) else {
                return;
            };
            let c = s
                .app_home
                .about
                .management
                .permissions
                .confirmation
                .take()
                .unwrap();
            let Some(lease) = read::Lease::begin(&mut s, visit) else {
                c.release.release();
                return;
            };
            document_mut(&mut s, visit).unwrap().pending = Some(id);
            s.app_home
                .about
                .management
                .permissions
                .attempts
                .push(Attempt {
                    replies: (0..plan.slots().len()).map(|_| None).collect(),
                    original,
                    lease,
                    plan,
                    copy_changed,
                    run_group,
                    manager_group: manager_changed.then(|| managers::Group::new(manager_plan)),
                    manager_source: c.managers,
                    departure: managers::departure::Departure::default(),
                    settings_source: c.settings,
                    finished: false,
                    complete: false,
                    resolved: false,
                    desired,
                    readback: None,
                    no_send: false,
                    release: Some(c.release),
                });
            true
        };
        if admitted {
            run::run(state, id).await;
        }
    });
}
fn exact(s: &ShellState, id: u64) -> bool {
    s.app_home
        .about
        .management
        .permissions
        .attempts
        .iter()
        .find(|a| a.original.id == id)
        .is_some_and(|a| {
            !a.finished
                && a.lease.current(s)
                && a.original.current(s, Some(id))
                && a.manager_source
                    .as_ref()
                    .is_none_or(|m| m.current(s, &a.original))
                && a.settings_source
                    .as_ref()
                    .is_none_or(|v| v.source_current(s, &a.original))
        })
}

async fn picker_read(
    state: Signal<ShellState>,
    request: &super_platinum_core::slack::PreparedRequest,
    managers: bool,
) -> Option<Result<Value, Error>> {
    if fixture::first_publish::requested() {
        fixture::first_publish::read(state, request).await
    } else if managers {
        fixture::managers::read(state, request).await
    } else {
        fixture::run_access::read(state, request).await
    }
}
async fn picker_request(
    state: Signal<ShellState>,
    request: &super_platinum_core::slack::PreparedRequest,
    managers: bool,
) -> Option<Result<(Value, Option<Vec<String>>), Error>> {
    if fixture::first_publish::requested() {
        fixture::first_publish::picker(state, request).await
    } else if managers {
        fixture::managers::picker(state, request).await
    } else {
        fixture::run_access::picker(state, request).await
    }
}
