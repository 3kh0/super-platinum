//! Data-only preparation. Short watches never become loaded-control leases.
use super::*;
use super_platinum_core::slack::{Error, PreparedRequest};
pub(super) struct Work {
    pub pending: Arc<Prepared>,
    watch: ReadWatch,
    dialog: u64,
    input: u64,
    shortcut: u64,
}
pub(super) fn begin(s: &mut ShellState, explicit: bool) -> Option<Work> {
    let context = Context::capture(s)?;
    let b = s.search_message_pages.as_ref()?;
    if let Some(p) = &b.profiles.prepared {
        if p.pending
            || p.blocked
            || p.retry_at.is_some_and(|t| t > std::time::Instant::now())
            || (!explicit && p.context.policy_current(s) && p.context.base(s))
        {
            return None;
        }
    }
    let roster = b.profiles.pages.get(&b.shown)?.clone();
    let actor = roster
        .actors
        .values()
        .find(|a| !a.retired.load(Ordering::Acquire))?;
    if context.cooling(s) {
        roster.attempted.store(true, Ordering::Release);
        roster.automatic.store(false, Ordering::Release);
        let (_, at, blocked) = s
            .search_author_profiles
            .cooldowns
            .get(&context.frame.team)?;
        let prepared = Arc::new(Prepared {
            context,
            roster,
            members: BTreeMap::new(),
            hidden: BTreeSet::new(),
            pending: false,
            failed: true,
            retry_at: *at,
            blocked: *blocked,
        });
        let state = &mut s.search_message_pages.as_mut()?.profiles;
        state.waiting = false;
        state.prepared = Some(prepared);
        return None;
    }
    let watch = context.watch(s, &actor.key.0)?;
    roster.attempted.store(true, Ordering::Release);
    roster.automatic.store(false, Ordering::Release);
    let pending = Arc::new(Prepared {
        context,
        roster,
        members: BTreeMap::new(),
        hidden: BTreeSet::new(),
        pending: true,
        failed: false,
        retry_at: None,
        blocked: false,
    });
    let work = Work {
        pending: pending.clone(),
        watch,
        dialog: s.search_open_generation,
        input: s.search_input_revision,
        shortcut: s.shortcut_generation,
    };
    let state = &mut s.search_message_pages.as_mut()?.profiles;
    state.waiting = false;
    state.prepared = Some(pending);
    Some(work)
}
impl Work {
    fn current(&self, s: &ShellState) -> bool {
        s.overlay == Some(Overlay::Search)
            && s.search_open_generation == self.dialog
            && s.search_input_revision == self.input
            && s.shortcut_generation == self.shortcut
            && self.pending.context.current(s)
            && self.watch.current()
            && s.search_message_pages
                .as_ref()
                .and_then(|b| b.profiles.prepared.as_ref())
                .is_some_and(|p| Arc::ptr_eq(p, &self.pending))
    }
}
pub(super) fn parse_policy(raw: &Value) -> Option<BTreeSet<String>> {
    if raw.get("ok") != Some(&Value::Bool(true)) || raw.get("error").is_some_and(|v| !v.is_null()) {
        return None;
    }
    let csv = raw
        .get("prefs")?
        .as_object()?
        .get("hidden_users")?
        .as_str()?;
    if csv.len() > 65536 {
        return None;
    }
    // Exact retained Source String domain; no trim/filter/normalization.
    Some(csv.split(',').map(str::to_owned).collect())
}
pub(super) fn humans(
    raw: &Value,
    ids: &[String],
    team: &str,
) -> Option<BTreeMap<String, super_platinum_core::profile_reading::Projection>> {
    Some(
        super_platinum_core::profile_reading::from_reply(raw, ids, team)
            .ok()?
            .into_iter()
            .filter(|p| !p.user().is_stranger && member_id(&p.user().id))
            .map(|p| (p.user().id.clone(), p))
            .collect(),
    )
}
fn unavailable() -> Error {
    Error::Api("search_author_unavailable".into())
}
fn cooling(e: &Error) -> (Option<std::time::Instant>, bool) {
    let seconds = match e {
        Error::RateLimited { retry_after_secs } => Some(retry_after_secs.unwrap_or(60)),
        Error::HttpStatus {
            retry_after_secs, ..
        } => *retry_after_secs,
        _ => None,
    };
    match seconds {
        Some(n) => match std::time::Instant::now().checked_add(std::time::Duration::from_secs(n)) {
            Some(t) => (Some(t), false),
            None => (None, true),
        },
        None => (None, false),
    }
}
// Observe completed attempts before the UI retirement fence. This retains only
// original-account cooldown data, never policy/member data or a new UI slot.
pub(super) fn observe_attempt(
    mut state: Signal<ShellState>,
    context: &Context,
    result: &Result<Value, Error>,
) {
    if let Err(error) = result
        && let Ok(mut s) = state.try_write()
    {
        context.note_failure(&mut s, error);
    }
}
async fn fetch(
    state: Signal<ShellState>,
    work: &Work,
    request: PreparedRequest,
    #[cfg(test)] io: &std::rc::Rc<test_io::Io>,
) -> Result<Value, Error> {
    if !work.current(&state.peek()) {
        return Err(unavailable());
    }
    #[cfg(test)]
    let result = io.execute(request).await;
    #[cfg(test)]
    observe_attempt(state, &work.pending.context, &result);
    #[cfg(not(test))]
    let result = work
        .pending
        .context
        .frame
        .transport
        .execute_admitted(
            request,
            || work.current(&state.peek()),
            || true,
            |result| observe_attempt(state, &work.pending.context, result),
        )
        .await
        .and_then(|value| value.ok_or_else(unavailable));
    if !work.current(&state.peek()) {
        return Err(unavailable());
    }
    result
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    work: Work,
    #[cfg(test)] io: Option<std::rc::Rc<test_io::Io>>,
) {
    #[cfg(test)]
    let Some(io) = io else { return };
    let _lease = Lease {
        state,
        pending: work.pending.clone(),
    };
    let result = async {
        let f = &work.pending.context.frame;
        let prefs = fetch(
            state,
            &work,
            super_platinum_core::block_actions::name_policy::request::user(&f.client, &f.session),
            #[cfg(test)]
            &io,
        )
        .await?;
        let hidden = parse_policy(&prefs).ok_or_else(unavailable)?;
        let ids = work
            .pending
            .roster
            .actors
            .values()
            .filter(|a| !a.retired.load(Ordering::Acquire) && !hidden.contains(&a.id))
            .map(|a| a.id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let members = if ids.is_empty() {
            BTreeMap::new()
        } else {
            if ids.len() > LIMIT {
                return Err(unavailable());
            }
            let request =
                super_platinum_core::slack::edge::profile_users_info(&f.client, &f.session, &ids)
                    .map_err(|_| unavailable())?;
            let raw = fetch(
                state,
                &work,
                request,
                #[cfg(test)]
                &io,
            )
            .await?;
            humans(&raw, &ids, &f.team).ok_or_else(unavailable)?
        };
        Ok((hidden, members))
    }
    .await;
    let Ok(mut s) = state.try_write() else { return };
    if let Err(error) = &result {
        work.pending.context.note_failure(&mut s, error);
    }
    if !work.current(&s) {
        // Release only this pending UI slot. Policy epoch/actor retirement remains.
        if let Some(b) = s.search_message_pages.as_mut() {
            if b.profiles
                .prepared
                .as_ref()
                .is_some_and(|p| Arc::ptr_eq(p, &work.pending))
            {
                b.profiles.prepared = Some(Arc::new(Prepared {
                    context: work.pending.context.clone(),
                    roster: work.pending.roster.clone(),
                    members: BTreeMap::new(),
                    hidden: BTreeSet::new(),
                    pending: false,
                    failed: true,
                    retry_at: None,
                    blocked: false,
                }));
            }
        }
        return;
    }
    let (hidden, members, failed, retry_at, blocked) = match result {
        Ok((h, m)) => (h, m, false, None, false),
        Err(e) => {
            let (t, b) = cooling(&e);
            (BTreeSet::new(), BTreeMap::new(), true, t, b)
        }
    };
    let prepared = Arc::new(Prepared {
        context: work.pending.context.clone(),
        roster: work.pending.roster.clone(),
        members,
        hidden,
        pending: false,
        failed,
        retry_at,
        blocked,
    });
    // All original/current checks above, no connection/owner read under this lock.
    let published = work.watch.with_current(|| {
        s.search_message_pages.as_mut().unwrap().profiles.prepared = Some(prepared);
    });
    if published.is_none() {
        fail_pending(&mut s, &work.pending);
    }
}

// A real negative/schema response is never relabeled as a transport outage.
pub(super) fn retain_failure(error: &Error) -> bool {
    match error {
        Error::Offline(_) | Error::Transport(_) | Error::RateLimited { .. } => true,
        Error::HttpStatus { status, .. } => *status >= 500 || matches!(*status, 408 | 429),
        Error::Api(code) => matches!(
            code.as_str(),
            "request_superseded" | "search_author_not_requested"
        ),
        _ => false,
    }
}

struct Lease {
    state: Signal<ShellState>,
    pending: Arc<Prepared>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.try_write() {
            fail_pending(&mut s, &self.pending);
        }
    }
}
