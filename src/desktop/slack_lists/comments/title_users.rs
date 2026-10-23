//! List-owned existing-token title acquisition. Query receipts are never reused.
use super::*;
use crate::directory_member_fetches::Context;
use crate::profile_edit::sections::rich::search::{
    FileTitles, TitleMemberSource, acquire_title_members, file_source,
};
use std::collections::BTreeMap;
use super_platinum_core::{
    block_actions::name_policy, state::posting_metadata::authority::Authority,
};
mod draft;
mod policy;
pub(crate) use draft::{labels, prepare_draft, retry_draft};
pub(super) use draft::{needs_mount, readiness};

#[derive(Clone, Default)]
pub(super) struct State {
    draft: Option<draft::Job>,
}
pub(crate) struct Observed {
    pub(super) files: Arc<FileTitles>,
    policy: Option<Arc<policy::Policy>>,
}
impl Observed {
    pub(crate) fn current(&self, s: &ShellState) -> bool {
        self.files.current(s) && self.policy.as_ref().is_none_or(|p| p.current(s))
    }
    pub(crate) fn snapshot(&self, s: &ShellState) -> Value {
        serde_json::json!({"current":self.current(s),"titles":self.files.snapshot(s),"policy":self.policy.as_ref().map(|p|p.snapshot())})
    }
}

/// A separate original own-edit Job supplies its Binding, seed, durable buffer
/// and reply-suspension guard. Reusing acquisition never reuses a reply receipt.
pub(super) async fn acquire_edit(
    state: Signal<ShellState>,
    team: &str,
    generation: &str,
    tokens: Vec<Value>,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
) -> Result<Arc<Observed>, Error> {
    acquire_reply(state, team, generation, tokens, owner).await
}

/// Original transcript occurrences explicitly own their application English title
/// context, independently of reply and own-edit mount observations.
pub(super) async fn acquire_transcript(
    state: Signal<ShellState>,
    team: &str,
    generation: &str,
    tokens: Vec<Value>,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
) -> Result<Arc<Observed>, Error> {
    acquire_with_context(
        state,
        team,
        generation,
        tokens,
        owner,
        Some(crate::profile_edit::sections::rich::title_locale::CanvasTitle::english()),
        false,
    )
    .await
}

/// The original saved reply owns this job independently of completion or transcript.
async fn acquire_reply(
    state: Signal<ShellState>,
    team: &str,
    generation: &str,
    tokens: Vec<Value>,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
) -> Result<Arc<Observed>, Error> {
    acquire_with_context(
        state,
        team,
        generation,
        tokens,
        owner,
        Some(crate::profile_edit::sections::rich::title_locale::CanvasTitle::english()),
        true,
    )
    .await
}

async fn acquire_with_context(
    mut state: Signal<ShellState>,
    team: &str,
    generation: &str,
    tokens: Vec<Value>,
    owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    canvas_locale: Option<crate::profile_edit::sections::rich::title_locale::CanvasTitle>,
    reply: bool,
) -> Result<Arc<Observed>, Error> {
    let context = Context::capture(&state.peek(), team)?;
    let base = owner.clone();
    let current = |s: &ShellState| base(s) && context.current(s);
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    // Only absent/stale metadata uses files.info. Nested-member Retry retains
    // the original accepted file row and cannot turn into a metadata refresh.
    let mut ids = std::collections::BTreeSet::new();
    for token in &tokens {
        let id = token["file_id"].as_str().ok_or_else(unavailable)?;
        ids.insert(id.to_owned());
    }
    for id in ids {
        if file_source::FileObservation::capture(&state.peek(), team, &id).is_ok()
            || (reply && FileTitles::reply_excluded(&state.peek(), team, &id))
        {
            continue;
        }
        let read = file_source::FileInfoRead::capture(&mut state.write(), team, &id)?;
        let scheduled =
            crate::scheduled::Context::capture(&state.peek()).ok_or_else(unavailable)?;
        let request = file_acquisition::request(&scheduled, &id);
        let raw = file_acquisition::run::acquire(state, request, scheduled, |s| {
            current(s) && read.current(s)
        })
        .await?;
        let mut s = state.write();
        if !current(&s) || !read.current(&s) {
            return Err(unavailable());
        }
        if let Err(error) = read.publish(&mut s, &raw) {
            // A positively observed unsupported category releases only the reply
            // label gate. Transport/malformed/unknown failures still require Retry.
            if !reply || !FileTitles::reply_excluded(&s, team, &id) {
                return Err(error);
            }
        }
    }
    let files = if reply {
        FileTitles::capture_reply(&state.peek(), team, tokens, owner)?
    } else {
        FileTitles::capture_with_context(&state.peek(), team, tokens, owner, canvas_locale)?
    };
    let (users, policy) = if files.required().is_empty() {
        (None, None)
    } else {
        let source = TitleMemberSource::capture(&state.peek(), team, generation, files.required())?;
        let admitted = |s: &ShellState| current(s) && files.source_current(s);
        let policy =
            Arc::new(policy::acquire(state, team, &|s| admitted(s) && source.current(s)).await?);
        let users = acquire_title_members(
            state,
            team,
            generation,
            |s| admitted(s) && policy.current(s),
            source,
            files.required(),
        )
        .await?;
        (users, Some(policy))
    };
    let p = policy.clone();
    let files = Arc::new(files.complete(
        &state.peek(),
        users,
        policy.as_ref().map(|p| p.names.clone()),
        Arc::new(move |s| p.as_ref().is_none_or(|p| p.current(s))),
    )?);
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(Arc::new(Observed { files, policy }))
}

pub(crate) fn snapshot(s: &ShellState) -> Value {
    draft::snapshot(s)
}
