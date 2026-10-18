//! A channel link belongs to one rendered Activity message occurrence. Neither
//! a repeated channel ID nor an unchanged execution ID transfers that authority.
use super::super::*;
use std::rc::Rc;
mod native;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app_home::about::management::activity) enum Location {
    Trigger,
    Step(usize),
}
impl Location {
    fn message(self, detail: &ActivityDetail) -> Option<&Value> {
        match self {
            Self::Trigger => detail.trigger_message(),
            Self::Step(index) => detail.steps().get(index)?.get("outcome_message"),
        }
    }
    fn button(self, part: usize) -> String {
        match self {
            Self::Trigger => format!("workflow-activity-channel-trigger-{part}"),
            Self::Step(index) => format!("workflow-activity-channel-step-{index}-{part}"),
        }
    }
}

#[derive(Clone)]
pub(super) struct Source {
    owner: Owner,
    policy: policy::Snapshot,
    credentials: String,
    visit: u64,
    document: u64,
    revision: u64,
    title: String,
    description: String,
    editor: Editor,
    pending: Option<u64>,
    filters: Filters,
    generation: u64,
    search: String,
    detail_revision: u64,
    detail: Value,
    row: Value,
    location: Location,
    part: usize,
    channel: String,
    profile: u64,
    thread: u64,
    palette: u64,
    search_open: u64,
    shortcut: u64,
    modal: Option<(u64, Option<String>)>,
}

fn modal(s: &ShellState) -> Option<(u64, Option<String>)> {
    let m = &s.app_modal.as_ref()?.modal;
    Some((m.revision(), m.stack().last().map(|v| v.id.clone())))
}

impl Source {
    pub(super) fn capture(
        s: &ShellState,
        visit: u64,
        location: Location,
        part: usize,
        channel: &str,
    ) -> Option<Self> {
        if !permitted(s, visit)
            || !channel
                .as_bytes()
                .first()
                .is_some_and(|c| matches!(c, b'C' | b'G' | b'D'))
            || !channel
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return None;
        }
        let h = s.app_home.about.management.surface.as_ref()?;
        let d = document(s, visit)?;
        let a = &d.activity;
        if h.read.is_some() || !a.active || a.detail_read.is_some() || a.member_composing {
            return None;
        }
        let detail = a.detail.as_ref()?;
        if presentation::parts(location.message(detail)?).get(part)
            != Some(&presentation::Part::Channel(channel.into()))
        {
            return None;
        }
        Some(Self {
            owner: h.owner.clone(),
            policy: h.policy.clone()?,
            credentials: serde_json::to_string(s.core.session.as_ref()?).ok()?,
            visit,
            document: d.id,
            revision: d.revision,
            title: d.title.clone(),
            description: d.description.clone(),
            editor: d.editor.clone(),
            pending: d.pending,
            filters: a.filters.clone(),
            generation: a.member_generation,
            search: a.user_search.clone(),
            detail_revision: a.detail_revision,
            detail: detail.raw().clone(),
            row: a.detail_row.clone()?,
            location,
            part,
            channel: channel.into(),
            profile: s.profile_generation,
            thread: s.thread_generation,
            palette: s.palette_generation,
            search_open: s.search_open_generation,
            shortcut: s.shortcut_generation,
            modal: modal(s),
        })
    }

    pub(super) fn current(&self, s: &ShellState) -> bool {
        if !permitted(s, self.visit) || !self.owner.current(s) {
            return false;
        }
        let Some(h) = s.app_home.about.management.surface.as_ref() else {
            return false;
        };
        let Some(d) = document(s, self.visit) else {
            return false;
        };
        let a = &d.activity;
        h.read.is_none()
            && a.active
            && a.detail_read.is_none()
            && !a.member_composing
            && h.policy.as_ref().is_some_and(|p| p.same(&self.policy))
            && s.core
                .session
                .as_ref()
                .and_then(|s| serde_json::to_string(s).ok())
                .as_ref()
                == Some(&self.credentials)
            && self.document == d.id
            && self.revision == d.revision
            && self.pending == d.pending
            && self.title == d.title
            && self.description == d.description
            && self.editor.responses() == d.editor.responses()
            && self.filters == a.filters
            && self.generation == a.member_generation
            && self.search == a.user_search
            && self.detail_revision == a.detail_revision
            && a.detail
                .as_ref()
                .is_some_and(|detail| detail.raw() == &self.detail)
            && a.detail_row.as_ref() == Some(&self.row)
            && self.profile == s.profile_generation
            && self.thread == s.thread_generation
            && self.palette == s.palette_generation
            && self.search_open == s.search_open_generation
            && self.shortcut == s.shortcut_generation
            && self.modal == modal(s)
    }

    pub(super) fn button(&self) -> String {
        self.location.button(self.part)
    }
    fn expected(&self, chain: u64) -> Value {
        json!({"visit":self.visit,"document":self.document,"revision":self.revision,
            "detail":self.detail_revision,"filters":self.filters,
            "generation":self.generation,"search":self.search,
            "button":self.button(),"channel":self.channel,"chain":chain})
    }
}

pub(super) fn open(state: Signal<ShellState>, source: Source) {
    if !source.current(&state.peek()) {
        return;
    }
    let source = Rc::new(source);
    let chain = next();
    let guard = crate::slack_links::NavigationGuard {
        current: {
            let source = source.clone();
            Rc::new(move |s| source.current(s))
        },
        lock: {
            let source = source.clone();
            Rc::new(move || {
                let expected = source.expected(chain);
                Box::pin(async move { native::lock(&expected).await })
            })
        },
    };
    let feedback: crate::slack_links::FailureFeedback = Some({
        let source = source.clone();
        Rc::new(move |shell, message| {
            if source.current(shell) {
                native::feedback(&source.expected(chain), message);
            }
        })
    });
    dioxus::core::spawn_forever(async move {
        crate::slack_links::open_channel_guarded(
            state,
            source.owner.ctx.session.team_id.clone(),
            source.channel.clone(),
            guard,
            feedback,
        )
        .await;
    });
}
