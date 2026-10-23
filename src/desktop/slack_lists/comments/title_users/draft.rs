use super::*;
#[derive(Clone)]
pub(super) struct Job {
    binding: super::super::draft::Binding,
    capture: String,
    sequence: u64,
    blocks: Vec<Value>,
    tokens: Vec<Value>,
    serial: Arc<()>,
    loading: bool,
    retry_at: i64,
    error: Option<String>,
    observed: Option<Arc<Observed>>,
}
impl Job {
    fn current(&self, s: &ShellState) -> bool {
        self.binding.current(s)
            && super::super::draft::entry(s, self.binding.id()).is_some_and(|e| {
                e.generation == self.binding.generation()
                    && e.capture == self.capture
                    && e.sequence == self.sequence
                    && e.blocks == self.blocks
            })
    }
    fn active(&self, s: &ShellState) -> bool {
        self.current(s)
            && s.slack_lists
                .comments
                .title_users
                .draft
                .as_ref()
                .is_some_and(|j| Arc::ptr_eq(&j.serial, &self.serial))
    }
}
pub(in crate::slack_lists::comments) fn readiness(
    s: &ShellState,
    b: &super::super::draft::Binding,
) -> (bool, bool, Option<String>) {
    let Some(e) = super::super::draft::entry(s, b.id()) else {
        return (false, false, None);
    };
    // A mounted editor owns its private labels and edits. Never remount/relabel
    // it because another metadata receipt changes after the original mount.
    if e.native.is_some() {
        return (true, false, None);
    }
    match s
        .slack_lists
        .comments
        .title_users
        .draft
        .as_ref()
        .filter(|j| j.binding.scope() == b.scope() && j.current(s))
    {
        Some(j) => (
            j.observed.as_ref().is_some_and(|o| o.current(s)),
            j.loading,
            j.error.clone().or_else(|| {
                (!j.loading && j.observed.as_ref().is_some_and(|o| !o.current(s)))
                    .then(|| "Title details changed. Retry before opening this draft.".into())
            }),
        ),
        None if FileTitles::reply_tokens(s, b.owner_team(), &e.blocks).is_empty() => {
            (true, false, None)
        }
        None => (
            false,
            false,
            Some("Title details are unavailable. Your saved draft is retained.".into()),
        ),
    }
}
pub(crate) fn labels(s: &ShellState, b: &super::super::draft::Binding) -> BTreeMap<String, String> {
    s.slack_lists
        .comments
        .title_users
        .draft
        .as_ref()
        .filter(|j| j.binding.scope() == b.scope() && j.current(s))
        .and_then(|j| j.observed.as_ref())
        .filter(|o| o.current(s))
        .and_then(|o| o.files.labels(s).ok())
        .unwrap_or_default()
}
pub(crate) fn prepare_draft(mut state: Signal<ShellState>, b: super::super::draft::Binding) {
    let job = {
        let s = state.peek();
        if !b.current(&s) {
            return;
        }
        let Some(e) = super::super::draft::entry(&s, b.id()) else {
            return;
        };
        if e.loading
            || e.inventory.is_none()
            || e.native.is_some()
            || FileTitles::reply_tokens(&s, b.owner_team(), &e.blocks).is_empty()
        {
            return;
        }
        if s.slack_lists
            .comments
            .title_users
            .draft
            .as_ref()
            .is_some_and(|j| j.binding.scope() == b.scope() && j.retry_at > now())
        {
            return;
        }
        if s.slack_lists
            .comments
            .title_users
            .draft
            .as_ref()
            .is_some_and(|j| {
                j.binding.scope() == b.scope()
                    && j.current(&s)
                    && (j.loading || j.observed.as_ref().is_some_and(|o| o.current(&s)))
            })
        {
            return;
        }
        Job {
            binding: b.clone(),
            capture: e.capture.clone(),
            sequence: e.sequence,
            blocks: e.blocks.clone(),
            tokens: FileTitles::reply_tokens(&s, b.owner_team(), &e.blocks),
            serial: Arc::new(()),
            loading: true,
            retry_at: 0,
            error: None,
            observed: None,
        }
    };
    state.write().slack_lists.comments.title_users.draft = Some(job.clone());
    dioxus::core::spawn_forever(async move {
        let owner = job.clone();
        let guard = Arc::new(move |s: &ShellState| owner.active(s));
        let generation = format!("list-draft-title-{}", job.binding.scope());
        let work = super::acquire_reply(
            state,
            job.binding.owner_team(),
            &generation,
            job.tokens.clone(),
            guard,
        );
        let result = file_acquisition::run::guarded(work, || job.active(&state.peek())).await;
        let mut s = state.write();
        if !job.active(&s) {
            return;
        }
        let slot = s.slack_lists.comments.title_users.draft.as_mut().unwrap();
        slot.loading = false;
        match result {
            Ok(o) => {
                slot.observed = Some(o);
                slot.error = None;
            }
            Err(error) => {
                slot.retry_at = match error {
                    Error::RateLimited { retry_after_secs }
                    | Error::HttpStatus {
                        status: 429,
                        retry_after_secs,
                    } => now().saturating_add(
                        i64::try_from(retry_after_secs.unwrap_or(30)).unwrap_or(i64::MAX),
                    ),
                    _ => 0,
                };
                slot.error = Some(
                    if slot.retry_at > now() {
                        "Slack asked us to wait before retrying title details."
                    } else {
                        "Title details could not load. Your saved draft is retained."
                    }
                    .into(),
                );
            }
        }
    });
}
pub(crate) fn retry_draft(mut state: Signal<ShellState>, b: super::super::draft::Binding) {
    let no_dependency = {
        let s = state.peek();
        b.current(&s)
            && super::super::draft::entry(&s, b.id()).is_some_and(|e| {
                e.native.is_none()
                    && !e.loading
                    && FileTitles::reply_tokens(&s, b.owner_team(), &e.blocks).is_empty()
            })
    };
    if no_dependency {
        state.write().slack_lists.comments.title_users.draft = None;
        return;
    }
    prepare_draft(state, b);
}
pub(super) fn snapshot(s: &ShellState) -> Value {
    s.slack_lists.comments.title_users.draft.as_ref().map(|j|serde_json::json!({"scope":j.binding.scope(),"capture":j.capture,"sequence":j.sequence,"blocks":j.blocks,"tokens":j.tokens,"current":j.current(s),"loading":j.loading,"retryAt":j.retry_at,"status":if j.loading{"loading"}else if j.observed.as_ref().is_some_and(|o|o.current(s)){"ready"}else{"failed"},"error":j.error,"observed":j.observed.as_ref().map(|o|o.snapshot(s))})).unwrap_or(Value::Null)
}

pub(in crate::slack_lists::comments) fn needs_mount(
    s: &ShellState,
    b: &super::super::draft::Binding,
) -> bool {
    s.slack_lists
        .comments
        .title_users
        .draft
        .as_ref()
        .is_some_and(|j| j.binding.scope() == b.scope())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn original(s: &ShellState, b: super::super::super::draft::Binding) -> Job {
        let e = super::super::super::draft::entry(s, b.id()).unwrap();
        Job {
            binding: b,
            capture: e.capture.clone(),
            sequence: e.sequence,
            blocks: e.blocks.clone(),
            tokens: Vec::new(),
            serial: Arc::new(()),
            loading: true,
            retry_at: 0,
            error: None,
            observed: None,
        }
    }
    #[test]
    fn pre_mount_title_job_refuses_replaced_record_capture_and_owner() {
        for mutation in 0..5 {
            let (mut s, b, _disk) = super::super::super::draft::tests::setup();
            let job = original(&s, b.clone());
            s.slack_lists.comments.title_users.draft = Some(job.clone());
            assert!(job.active(&s));
            match mutation {
                0 => s.core.account_epoch += 1,
                1 => s.slack_lists.comments.drafts.active = None,
                2 => super::super::super::draft::entry_mut(&mut s, b.id())
                    .unwrap()
                    .capture
                    .push_str("replacement"),
                3 => super::super::super::draft::entry_mut(&mut s, b.id())
                    .unwrap()
                    .blocks
                    .push(serde_json::json!({"type":"text","text":"new"})),
                _ => {
                    s.slack_lists
                        .comments
                        .title_users
                        .draft
                        .as_mut()
                        .unwrap()
                        .serial = Arc::new(())
                }
            }
            assert!(!job.active(&s));
        }
    }
    #[test]
    fn cold_canvas_waits_for_own_reply_receipt_and_failure_preserves_exact_saved_blocks() {
        let (mut s, b, _disk) = super::super::super::draft::tests::setup();
        let raw = serde_json::json!({"type":"canvas","file_id":"FCOLD01","text":"Original fallback",
            "url":"https://fixture.invalid/raw","style":{"bold":true},"opaque":17});
        let blocks = vec![
            serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[raw]}]}),
        ];
        super::super::super::draft::entry_mut(&mut s, b.id())
            .unwrap()
            .blocks = blocks.clone();
        let capture = super::super::super::draft::entry(&s, b.id())
            .unwrap()
            .capture
            .clone();
        assert_eq!(
            readiness(&s, &b),
            (
                false,
                false,
                Some("Title details are unavailable. Your saved draft is retained.".into())
            )
        );
        let mut job = original(&s, b.clone());
        job.tokens = FileTitles::reply_tokens(&s, b.owner_team(), &blocks);
        assert_eq!(job.tokens, vec![raw]);
        s.slack_lists.comments.title_users.draft = Some(job.clone());
        assert_eq!(readiness(&s, &b), (false, true, None));
        job.loading = false;
        job.error = Some("Title details could not load. Your saved draft is retained.".into());
        s.slack_lists.comments.title_users.draft = Some(job);
        assert_eq!(
            readiness(&s, &b),
            (
                false,
                false,
                Some("Title details could not load. Your saved draft is retained.".into())
            )
        );
        assert!(labels(&s, &b).is_empty());
        let retained = super::super::super::draft::entry(&s, b.id()).unwrap();
        assert_eq!(retained.blocks, blocks);
        assert_eq!(retained.capture, capture);
        assert!(retained.native.is_none());
    }
}
