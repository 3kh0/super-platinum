use super::*;
use protocol::confirmation_add::{Add, Added, Bookmarks, Shortcut};
use std::rc::Rc;

pub(super) struct Operation {
    generation: u64,
    channel: String,
    request: Option<Value>,
    bookmarks: Option<Value>,
    raw: Option<Value>,
    normalized: Option<Value>,
    outcome: &'static str,
    finished: bool,
    navigated: bool,
    canonical: bool,
}
impl Operation {
    pub(super) fn snapshot(&self) -> Value {
        json!({"generation":self.generation,"channel":self.channel,"request":self.request,"bookmarks":self.bookmarks,"raw":self.raw,"normalized":self.normalized,"outcome":self.outcome,"finished":self.finished,"navigated":self.navigated,"canonical":self.canonical})
    }
}
#[derive(Clone, PartialEq)]
struct BookmarkStamp {
    scope: Option<(u64, String, String)>,
    generation: u64,
    rows: Vec<super_platinum_core::bookmarks::Bookmark>,
}
impl BookmarkStamp {
    fn capture(s: &ShellState) -> Option<Self> {
        let b = &s.core.bookmarks;
        (!b.busy && !b.loading && b.draft.is_none()).then(|| Self {
            scope: b.scope.clone(),
            generation: b.generation,
            rows: b.items.clone(),
        })
    }
    fn current(&self, s: &ShellState) -> bool {
        Self::capture(s).as_ref() == Some(self)
    }
}
pub(super) fn submit(mut state: Signal<ShellState>, visit: u64, id: u64) {
    let Some(q) = Query::capture(&state.peek(), visit, id) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        if !native::current(&q, Some(&json!({"kind":"done"}))).await || !q.current(&state.peek()) {
            return;
        }
        let selected = q.expected["selected"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if selected.is_empty() {
            super::super::readback::close_current(&mut state.write(), visit, id, None);
            return;
        }
        {
            let mut s = state.write();
            if !q.current(&s) {
                return;
            }
            let v = attempt_mut(&mut s, id)
                .unwrap()
                .confirmation_add
                .as_mut()
                .unwrap();
            if v.operation.is_some() || v.loading {
                return;
            }
            v.operation = Some(Operation {
                generation: q.generation,
                channel: selected.clone(),
                request: None,
                bookmarks: None,
                raw: None,
                normalized: None,
                outcome: "preflight",
                finished: false,
                navigated: false,
                canonical: false,
            });
        }
        let result = prepare(state, &q, &selected).await;
        let (add, choice, list, stamp) = match result {
            Ok(v) => v,
            Err(error) => {
                settle(
                    state,
                    &q,
                    None,
                    None,
                    if matches!(&error,Error::Api(c) if c=="workflow_confirmation_duplicate_shortcut")
                    {
                        "duplicate"
                    } else {
                        "not-sent"
                    },
                );
                finish(state, &q, false).await;
                return;
            }
        };
        if !q.authorized(state).await
            || !choice.current(&state.peek())
            || !stamp.current(&state.peek())
        {
            settle(state, &q, None, None, "not-sent");
            finish(state, &q, false).await;
            return;
        }
        let request = add.request(&q.ctx().client, &q.ctx().session);
        let admitted = {
            let mut s = state.write();
            if q.current(&s) && choice.current(&s) && stamp.current(&s) {
                let op = operation(&mut s, &q).unwrap();
                op.request =
                    Some(json!({"method":add.plan().method(),"arguments":add.plan().arguments()}));
                op.bookmarks = Some(list.raw().clone());
                op.outcome = "in-flight";
                true
            } else {
                false
            }
        };
        if !admitted {
            settle(state, &q, None, None, "not-sent");
            finish(state, &q, false).await;
            return;
        }
        // The dispatched request is never cancelled/replayed by modal retirement.
        // Its actual outcome stays attached to the original operation.
        let result = if fixture::first_publish::confirmation::requested() {
            fixture::first_publish::confirmation::mutation(state, &request)
                .await
                .unwrap_or_else(|| Err(unavailable()))
        } else if crate::fixture::is_fixture() {
            Err(Error::Api("workflow_management_not_sent_fixture".into()))
        } else {
            q.ctx()
                .transport
                .execute_workflow_confirmation_add(request)
                .await
        };
        let (raw, accepted, outcome) = match result {
            Ok(raw) => {
                let accepted = add.acknowledge(raw.clone()).ok();
                let outcome = if accepted.is_some() {
                    "acknowledged"
                } else if raw["ok"] == false {
                    "rejected"
                } else {
                    "unknown"
                };
                (Some(raw), accepted, outcome)
            }
            Err(Error::Offline(_)) => (None, None, "not-sent"),
            Err(Error::Api(c)) if c.starts_with("workflow_management_not_sent_") => {
                (None, None, "not-sent")
            }
            Err(_) => (None, None, "unknown"),
        };
        settle(state, &q, raw, accepted.as_ref(), outcome);
        let Some(added) = accepted else {
            finish(state, &q, false).await;
            return;
        };
        if !q.authorized(state).await
            || !choice.current(&state.peek())
            || !stamp.current(&state.peek())
        {
            finish(state, &q, false).await;
            return;
        }
        let Some(items) = canonical_rows(&list, &added) else {
            finish(state, &q, false).await;
            return;
        };
        let applied = {
            let mut s = state.write();
            if q.current(&s) && choice.current(&s) && stamp.current(&s) {
                let epoch = s.core.account_epoch;
                let store = &mut s.core.bookmarks;
                store.ensure_scope(epoch, &q.ctx().session.team_id, added.channel());
                let applied = store.begin().is_some_and(|g| store.apply(g, items));
                operation(&mut s, &q).unwrap().canonical = applied;
                applied
            } else {
                false
            }
        };
        if !applied {
            finish(state, &q, false).await;
            return;
        }
        if !added.navigable() {
            finish(state, &q, false).await;
            return;
        }
        let query = q.clone();
        let source = choice.clone();
        let lock = q.clone();
        let guard = crate::slack_links::NavigationGuard {
            current: Rc::new(move |s| query.current(s) && source.current(s)),
            lock: Rc::new(move || {
                let q = lock.clone();
                Box::pin(async move { native::lock(&q).await })
            }),
            read: None,
        };
        let generation = state.peek().channel_generation;
        crate::slack_links::open_channel_workflows_guarded(
            state,
            q.ctx().session.team_id.clone(),
            selected.clone(),
            guard,
            Some(Rc::new(|_, _| {})),
        )
        .await;
        let navigated = {
            let s = state.peek();
            q.ctx().current(&s)
                && s.channel_generation == generation.wrapping_add(1)
                && s.core.active_channel.as_deref() == Some(selected.as_str())
                && s.channel_tab == crate::state::ChannelTab::Workflows
        };
        if let Some(op) = operation(&mut state.write(), &q) {
            op.navigated = navigated;
        }
        finish(state, &q, navigated).await;
    });
}
async fn prepare(
    state: Signal<ShellState>,
    q: &Query,
    selected: &str,
) -> Result<(Add, channels::Choice, Bookmarks, BookmarkStamp), Error> {
    let stamp = BookmarkStamp::capture(&state.peek()).ok_or_else(unavailable)?;
    let choice = channels::validate(state, q, selected).await?;
    let plan = Bookmarks::request(selected)?;
    let raw = q
        .read(state, plan.request(&q.ctx().client, &q.ctx().session))
        .await?;
    let list = Bookmarks::decode(selected, raw)?;
    let s = state.peek();
    if !q.current(&s) || !choice.current(&s) || !stamp.current(&s) {
        return Err(unavailable());
    }
    let trigger = attempt(&s, q.id)
        .and_then(|a| a.followup.as_ref())
        .and_then(|f| f.current_trigger())
        .ok_or_else(unavailable)?;
    let shortcut = Shortcut::capture(trigger)?;
    let flags = choice.flags(&s).ok_or_else(unavailable)?;
    let (team, top) = choice.scope(&s).ok_or_else(unavailable)?;
    let add = Add::new(&shortcut, choice.raw(), flags, &list, &team, &top)?;
    Ok((add, choice, list, stamp))
}
fn operation<'a>(s: &'a mut ShellState, q: &Query) -> Option<&'a mut Operation> {
    attempt_mut(s, q.id)?
        .confirmation_add
        .as_mut()?
        .operation
        .as_mut()
        .filter(|op| op.generation == q.generation)
}
fn settle(
    mut state: Signal<ShellState>,
    q: &Query,
    raw: Option<Value>,
    added: Option<&Added>,
    outcome: &'static str,
) {
    if let Some(op) = operation(&mut state.write(), q) {
        op.raw = raw;
        op.normalized = added.map(|a| a.bookmark().clone());
        op.outcome = outcome;
        op.finished = true;
    }
}
fn canonical_rows(
    list: &Bookmarks,
    added: &Added,
) -> Option<Vec<super_platinum_core::bookmarks::Bookmark>> {
    let mut rows = list
        .rows()
        .iter()
        .map(|r| serde_json::from_value::<super_platinum_core::bookmarks::Bookmark>(r.clone()).ok())
        .collect::<Option<Vec<_>>>()?;
    let row = serde_json::from_value::<super_platinum_core::bookmarks::Bookmark>(
        added.raw_bookmark().clone(),
    )
    .ok()?;
    rows.retain(|r| r.id != row.id);
    rows.push(row);
    Some(rows)
}
async fn finish(mut state: Signal<ShellState>, q: &Query, navigated: bool) {
    if q.current(&state.peek()) && native::current(q, None).await && q.current(&state.peek()) {
        let mut s = state.write();
        let message=attempt(&s,q.id).and_then(|a|a.confirmation_add.as_ref()).and_then(|v|v.operation.as_ref()).map(|op|match op.outcome{
            "duplicate"=>"This workflow is already in that channel. No bookmark was added. Your original draft is retained.",
            "acknowledged"=>"The workflow bookmark was added. Your original draft is retained.",
            "rejected"=>"Slack rejected adding this workflow to the channel. Your original draft is retained.",
            "not-sent"=>"The workflow was not added to the channel. Your original draft is retained.",
            _=>"Adding the workflow could not be confirmed. Nothing will be replayed. Your original draft is retained.",
        });
        super::super::readback::close_current(&mut s, q.original.visit, q.id, message);
        return;
    }
    // Retiring this original receipt/lease never renders into a successor's
    // document. Navigation is already complete, or unrelated retirement won.
    let mut s = state.write();
    let baseline = attempt(&s, q.id)
        .filter(|a| a.resolved)
        .and_then(|a| a.readback.as_ref())
        .map(|(e, _)| e.clone());
    // The only document we may touch after intentional navigation is the exact
    // now-unmounted original. Title/description remain its independent draft.
    if navigated && q.ctx().current(&s) && s.core.active_account == q.account {
        if let Some(d) = s.app_home.about.management.documents.iter_mut().find(|d| {
            d.id == q.original.document
                && d.account == q.account
                && d.team == q.ctx().session.team_id
                && d.user == q.ctx().session.user_id
                && d.revision == q.original.revision
                && d.pending == Some(q.id)
                && d.title == q.original.title
                && d.description == q.original.description
                && d.editor.same_revision(&q.original.editor)
        }) {
            if let Some(editor) = baseline {
                d.editor = editor;
                d.pending = None;
                d.observed = None;
            }
            d.notice = Some(
                "The workflow bookmark was added. Your original metadata draft is retained.".into(),
            );
            d.revision = next();
        }
    }
    if let Some(a) = attempt_mut(&mut s, q.id) {
        a.closed = true;
        if let Some(op) = a
            .confirmation_add
            .as_mut()
            .and_then(|v| v.operation.as_mut())
        {
            op.finished = true;
            op.navigated = navigated;
        }
        if let Some(release) = a.release.take() {
            release.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bookmark_custody_refuses_pending_edit_or_changed_scope_generation_and_rows() {
        let (mut s, _, _) = super::super::super::super::tests::prepared();
        s.core.bookmarks = Default::default();
        s.core.bookmarks.ensure_scope(1, "T1", "C1");
        let original = BookmarkStamp::capture(&s).unwrap();
        assert!(original.current(&s));
        s.core.bookmarks.busy = true;
        assert!(BookmarkStamp::capture(&s).is_none());
        s.core.bookmarks.busy = false;
        s.core.bookmarks.loading = true;
        assert!(!original.current(&s));
        s.core.bookmarks.loading = false;
        s.core.bookmarks.draft = Some(super_platinum_core::bookmarks::Draft::new(None, false));
        assert!(!original.current(&s));
        s.core.bookmarks.draft = None;
        s.core
            .bookmarks
            .items
            .push(super_platinum_core::bookmarks::Bookmark {
                id: "BkOriginal".into(),
                ..Default::default()
            });
        assert!(!original.current(&s));
        s.core.bookmarks.items.clear();
        assert!(original.current(&s));
        s.core.bookmarks.generation += 1;
        assert!(!original.current(&s));
        s.core.bookmarks.generation = original.generation;
        s.core.bookmarks.ensure_scope(1, "T1", "C2");
        assert!(!original.current(&s));
    }
    #[test]
    fn canonical_upsert_preserves_shortcut_and_unrelated_raw_rows() {
        let channel =
            json!({"id":"CADD01","is_channel":true,"is_member":true,"fromAnotherTeam":false});
        let trigger=Shortcut::capture(&json!({"id":"FtABC","type":"shortcut","name":"N","shortcut_url":"https://slack.com/shortcuts/x"})).unwrap();
        let list = Bookmarks::decode(
            "CADD01",
            json!({"ok":true,"bookmarks":[{"id":"BkOld","type":"link","title":"old"}]}),
        )
        .unwrap();
        let add = Add::new(
            &trigger,
            &channel,
            super_platinum_core::block_actions::posting_policy::Flags {
                read_only: false,
                thread_only: false,
            },
            &list,
            "T1",
            "T1",
        )
        .unwrap();
        let accepted=add.acknowledge(json!({"ok":true,"bookmark":{"id":"BkNew","channel_id":"CADD01","shortcut_id":"FtABC","type":"shortcut","title":"N"}})).unwrap();
        let rows = canonical_rows(&list, &accepted).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "BkOld");
        assert_eq!(rows[1].shortcut_id.as_deref(), Some("FtABC"));
    }
}
