//! Resolve a selected canonical control through actual read results. A missing
//! root is not permission to create one; this route never sends Qpc or a write.
use super::*;

pub(super) struct Pending {
    owner: Owner,
    control: String,
}

/// Read-only routing custody; never a write permit or a replacement root.
pub(super) struct Selected {
    owner: Owner,
    control: String,
    ts: String,
    verified: bool,
}

pub(super) fn clear(value: &mut State) {
    if value.selected_control.is_some() || value.selected_thread.is_some() {
        // Explicit Show all supersedes the old read even if a stale callback
        // arrives while loading; that callback must not project without custody.
        value.next_serial();
        value.loading = false;
    }
    value.selected_control = None;
    value.selected_thread = None;
}

fn matches_root(selected: &Selected, message: &Message) -> bool {
    message.ts.as_deref() == Some(selected.ts.as_str())
        && message.subtype.as_deref() == Some("document_comment_root")
        && message
            .thread_ts
            .as_deref()
            .is_none_or(|ts| ts == selected.ts)
        && message
            .extra
            .get("document_comment")
            .and_then(|v| v.get("thread_id"))
            .and_then(serde_json::Value::as_str)
            == Some(selected.control.as_str())
}

pub(super) fn reply_ready(value: &State) -> bool {
    value.selected_thread.as_ref().is_none_or(|selected| {
        selected.verified
            && !value.loading
            && value.error.is_none()
            && value.query == Some(Query::Replies(selected.ts.clone()))
            && value
                .root
                .as_ref()
                .is_some_and(|root| matches_root(selected, root))
    })
}

/// The exact first-page admission used by begin. A rejected selected read may
/// have no projected Message; its retained original identity permits a fresh
/// read only. The root stays absent until the matching actual page is accepted.
pub(super) fn begin_replies(shell: &ShellState, value: &mut State, root: &str) -> bool {
    let selected = value
        .rows
        .iter()
        .find(|row| row.message.ts.as_deref() == Some(root))
        .map(|row| row.message.clone())
        .or_else(|| {
            value
                .root
                .as_ref()
                .filter(|message| message.ts.as_deref() == Some(root))
                .cloned()
        });
    if selected.is_none()
        && !value.selected_thread.as_ref().is_some_and(|bound| {
            bound.owner.current(shell)
                && bound.ts == root
                && value.query == Some(Query::Replies(bound.ts.clone()))
        })
    {
        return false;
    }
    value.root = selected;
    true
}

pub(super) fn admit_page(shell: &ShellState, value: &State, ticket: &Ticket, page: &Page) -> bool {
    let Some(selected) = &value.selected_thread else {
        return true;
    };
    if !value.accepts(shell, ticket)
        || !selected.owner.current(shell)
        || ticket.query != Query::Replies(selected.ts.clone())
    {
        return false;
    }
    let Page::Replies(page) = page else {
        return false;
    };
    let mut roots = page
        .messages
        .iter()
        .filter(|m| m.ts.as_deref() == Some(selected.ts.as_str()));
    if let Some(root) = roots.next() {
        return roots.next().is_none() && matches_root(selected, root);
    }
    // Valid subsequent pages need not repeat the root. They may rely only on
    // this selection's previously verified actual root, never on a stream row.
    ticket.cursor.is_some()
        && selected.verified
        && value
            .root
            .as_ref()
            .is_some_and(|root| matches_root(selected, root))
}

pub(super) fn accepted(value: &mut State, ticket: &Ticket) {
    if let Some(selected) = &mut value.selected_thread {
        if ticket.query == Query::Replies(selected.ts.clone()) {
            selected.verified = true;
        }
    }
}

pub(super) fn refuse(value: &mut State) {
    if let Some(selected) = &mut value.selected_thread {
        selected.verified = false;
    }
    value.rows.clear();
    value.root = None;
    value.cursor = None;
    value.pages = Default::default();
    value.loading = false;
    value.mutation_ready = false;
    value.review = None;
    value.error = Some(
        "The returned thread does not match the selected comment. Refresh comments before continuing.",
    );
}

fn root<'a>(
    messages: impl Iterator<Item = &'a Message>,
    control: &str,
) -> Result<Option<String>, ()> {
    let mut found = None;
    for message in messages {
        if message
            .extra
            .get("document_comment")
            .and_then(|v| v.get("thread_id"))
            .and_then(serde_json::Value::as_str)
            != Some(control)
        {
            continue;
        }
        let ts = message
            .ts
            .as_deref()
            .filter(|ts| super_platinum_core::canvas::comments::timestamp(ts))
            .ok_or(())?;
        if message.subtype.as_deref() != Some("document_comment_root")
            || message
                .thread_ts
                .as_deref()
                .is_some_and(|parent| parent != ts)
            || found.replace(ts.to_owned()).is_some()
        {
            return Err(());
        }
    }
    Ok(found)
}

pub(super) fn open(
    mut state: Signal<ShellState>,
    mut model: Signal<State>,
    owner: Owner,
    control: String,
) {
    if !owner.current(&state.peek()) || control.is_empty() || control.len() > 256 {
        return;
    }
    cancel_pending_link(&mut state.write());
    // A deliberate current selection supersedes only the view's read. Original
    // reply/resolve/annotation operations live in their separate retained roots.
    {
        let shell = state.peek();
        let mut value = model.write();
        if value
            .owner
            .as_ref()
            .is_some_and(|previous| !previous.current(&shell))
        {
            *value = State::default();
        }
        value.next_serial();
        value.loading = false;
        value.owner = Some(owner.clone());
        value.selected_thread = None;
        value.selected_control = Some(Pending { owner, control });
    }
    start(state, model, Query::Stream(Filter::Recent), false);
}

pub(super) fn before_read(shell: &ShellState, value: &mut State, query: &Query) {
    if value.selected_thread.as_ref().is_some_and(|selected| {
        !selected.owner.current(shell) || query != &Query::Replies(selected.ts.clone())
    }) {
        value.selected_thread = None;
    }
    if !matches!(query, Query::Stream(Filter::Recent))
        || value
            .selected_control
            .as_ref()
            .is_some_and(|pending| !pending.owner.current(shell))
    {
        value.selected_control = None;
    }
}

pub(super) fn after_read(state: Signal<ShellState>, mut model: Signal<State>, ticket: &Ticket) {
    let found = {
        let shell = state.peek();
        let value = model.peek();
        if !value.accepts(&shell, ticket)
            || value.loading
            || value.error.is_some()
            || !matches!(ticket.query, Query::Stream(Filter::Recent))
        {
            return;
        }
        let Some(pending) = &value.selected_control else {
            return;
        };
        if !pending.owner.current(&shell) {
            return;
        }
        root(value.rows.iter().map(|row| &row.message), &pending.control)
    };
    match found {
        Ok(Some(ts)) => {
            {
                let mut value = model.write();
                let Some(pending) = value.selected_control.take() else {
                    return;
                };
                value.selected_thread = Some(Selected {
                    owner: pending.owner,
                    control: pending.control,
                    ts: ts.clone(),
                    verified: false,
                });
            }
            // begin reuses the actual row, then performs the unchanged numeric
            // replies read. No synthetic Message, timestamp or write authority.
            start(state, model, Query::Replies(ts), false);
        }
        Ok(None) if model.peek().cursor.is_some() => {}
        Ok(None) => {
            model.write().error = Some(
                "The selected comment was not found in the current comments. Refresh or show all threads; no new comment was created.",
            )
        }
        Err(()) => {
            model.write().error = Some(
                "Slack returned conflicting identities for the selected comment. Refresh comments before continuing.",
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell() -> ShellState {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        shell.canvas = Some(crate::state::CanvasTarget {
            id: "Ffixture".into(),
            title: "Fixture".into(),
            permalink: None,
        });
        shell.overlay = Some(Overlay::Canvas);
        shell
    }
    fn pending(shell: &ShellState) -> State {
        let owner = Owner::capture(shell).unwrap();
        State {
            owner: Some(owner.clone()),
            selected_control: Some(Pending {
                owner,
                control: "old".into(),
            }),
            ..Default::default()
        }
    }
    fn message(ts: &str, control: &str) -> Message {
        serde_json::from_value(serde_json::json!({
            "type":"message", "subtype":"document_comment_root", "ts":ts,
            "document_comment":{"thread_id":control}
        }))
        .unwrap()
    }
    #[test]
    fn actual_root_identity_is_required_and_control_is_never_a_timestamp() {
        let messages = [
            message("1700000000.000001", "other"),
            message("1700000000.000002", "old"),
        ];
        assert_eq!(
            root(messages.iter(), "old").unwrap(),
            Some("1700000000.000002".into())
        );
        assert_eq!(root(messages.iter(), "missing").unwrap(), None);
        assert!(root([message("Qpc:old", "old")].iter(), "old").is_err());
    }
    #[test]
    fn conflicting_root_reply_or_parent_refuses_without_first_match_fallback() {
        let first = message("1700000000.000001", "old");
        assert!(
            root(
                [first.clone(), message("1700000000.000002", "old")].iter(),
                "old"
            )
            .is_err()
        );
        let mut reply = first.clone();
        reply.subtype = None;
        assert!(root([reply].iter(), "old").is_err());
        let mut foreign = first;
        foreign.thread_ts = Some("1700000000.000009".into());
        assert!(root([foreign].iter(), "old").is_err());
    }

    #[test]
    fn actual_owner_visit_and_query_changes_release_only_lookup_custody() {
        let mut shell = shell();
        let mut value = pending(&shell);
        before_read(&shell, &mut value, &Query::Stream(Filter::Recent));
        assert!(value.selected_control.is_some());
        before_read(&shell, &mut value, &Query::Stream(Filter::Resolved));
        assert!(value.selected_control.is_none());
        value = pending(&shell);
        shell.shortcut_generation += 1;
        before_read(&shell, &mut value, &Query::Stream(Filter::Recent));
        assert!(value.selected_control.is_none());
        value = pending(&shell);
        shell.core.account_epoch += 1;
        before_read(&shell, &mut value, &Query::Stream(Filter::Recent));
        assert!(value.selected_control.is_none());
    }

    #[test]
    fn read_result_routes_only_through_actual_numeric_replies_form() {
        let shell = shell();
        let page = StreamPage::parse(
            "Ffixture",
            &serde_json::json!({
                "ok":true,"messages":[message("1700000000.000001", "old")],"has_more":false
            }),
        )
        .unwrap();
        let found = root(page.threads.iter().map(|thread| &thread.root), "old")
            .unwrap()
            .unwrap();
        let ticket = Ticket {
            owner: Owner::capture(&shell).unwrap(),
            serial: 1,
            query: Query::Replies(found),
            cursor: None,
            direction: Direction::First,
        };
        let session = super_platinum_core::config::WorkspaceSession {
            team_id: ticket.owner.scope.team.clone(),
            user_id: shell.self_account.user_id.clone(),
            enterprise_id: None,
            name: "Fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let request = super::super::request(&shell.core.client, &session, &ticket).unwrap();
        assert_eq!(
            url::Url::parse(&request.url).unwrap().path(),
            "/api/conversations.replies"
        );
        let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
            panic!("expected actual form")
        };
        assert!(
            fields
                .iter()
                .any(|(key, value)| key == "ts" && value == "1700000000.000001")
        );
        assert!(
            !fields
                .iter()
                .any(|(key, value)| key == "content_snippet" || value.starts_with("Qpc:"))
        );
    }

    fn selected(shell: &ShellState) -> (State, Ticket) {
        let owner = Owner::capture(shell).unwrap();
        let ts = "1700000000.000001".to_owned();
        let ticket = Ticket {
            owner: owner.clone(),
            serial: 10,
            query: Query::Replies(ts.clone()),
            cursor: None,
            direction: Direction::First,
        };
        let value = State {
            owner: Some(owner.clone()),
            serial: 10,
            query: Some(ticket.query.clone()),
            selected_thread: Some(Selected {
                owner,
                control: "old".into(),
                ts: ts.clone(),
                verified: false,
            }),
            root: Some(message(&ts, "old")),
            loading: true,
            ..Default::default()
        };
        (value, ticket)
    }
    fn replies(message: Message, initial: bool) -> Page {
        Page::Replies(
            ReplyPage::parse(
                "Ffixture",
                "1700000000.000001",
                initial,
                &serde_json::json!({"ok":true,"messages":[message],"has_more":false}),
            )
            .unwrap(),
        )
    }
    #[test]
    fn actual_reply_parser_changed_control_is_rejected_before_projection() {
        let shell = shell();
        let (mut value, ticket) = selected(&shell);
        assert!(!reply_ready(&value));
        // The ordinary parser accepts this numeric root. Selection routing must
        // additionally retain the original annotation identity across the read.
        let wrong = replies(message("1700000000.000001", "other"), true);
        assert!(!admit_page(&shell, &value, &ticket, &wrong));
        assert!(!value.selected_thread.as_ref().unwrap().verified);
        let mut missing = message("1700000000.000001", "old");
        missing.extra.remove("document_comment");
        assert!(
            ReplyPage::parse(
                "Ffixture",
                "1700000000.000001",
                true,
                &serde_json::json!({"ok":true,"messages":[missing.clone()]})
            )
            .is_err()
        );
        assert!(!admit_page(
            &shell,
            &value,
            &ticket,
            &Page::Replies(ReplyPage {
                messages: vec![missing],
                cursor: None
            })
        ));
        let correct = replies(message("1700000000.000001", "old"), true);
        assert!(admit_page(&shell, &value, &ticket, &correct));
        accepted(&mut value, &ticket);
        value.loading = false;
        assert!(reply_ready(&value));
        refuse(&mut value);
        assert!(value.root.is_none() && value.rows.is_empty() && !value.mutation_ready);
        assert!(!reply_ready(&value));
        assert!(value.selected_thread.is_some()); // explicit refresh can recover
        assert!(admit_page(&shell, &value, &ticket, &correct));
    }
    #[test]
    fn later_pages_rely_only_on_verified_root_and_recheck_any_supplied_root() {
        let shell = shell();
        let (mut value, mut ticket) = selected(&shell);
        let mut reply = message("1700000000.000002", "old");
        reply.subtype = None;
        reply.thread_ts = Some("1700000000.000001".into());
        let later = replies(reply, false);
        assert!(!admit_page(&shell, &value, &ticket, &later));
        ticket.cursor = Some("next".into());
        ticket.direction = Direction::Next;
        assert!(!admit_page(&shell, &value, &ticket, &later));
        accepted(&mut value, &ticket);
        assert!(admit_page(&shell, &value, &ticket, &later));
        assert!(!admit_page(
            &shell,
            &value,
            &ticket,
            &replies(message("1700000000.000001", "other"), false)
        ));
        value.root = Some(message("1700000000.000001", "other"));
        assert!(!admit_page(&shell, &value, &ticket, &later));
        value.root = Some(message("1700000000.000001", "old"));
        ticket.cursor = None;
        ticket.direction = Direction::First;
        assert!(!admit_page(&shell, &value, &ticket, &later));
    }
    #[test]
    fn selected_reply_refresh_preserves_binding_and_explicit_navigation_releases_it() {
        let shell = shell();
        let (mut value, ticket) = selected(&shell);
        before_read(&shell, &mut value, &ticket.query);
        assert!(value.selected_thread.is_some());
        before_read(
            &shell,
            &mut value,
            &Query::Replies("1700000000.000009".into()),
        );
        assert!(value.selected_thread.is_none());
        value = selected(&shell).0;
        before_read(&shell, &mut value, &Query::Stream(Filter::Recent));
        assert!(value.selected_thread.is_none());
        value = selected(&shell).0;
        clear(&mut value);
        assert!(value.selected_thread.is_none() && value.selected_control.is_none());
        assert!(!value.accepts(&shell, &ticket));
        assert!(!value.loading);
    }
    #[test]
    fn selected_reply_old_serial_and_retired_owner_never_admit_or_rebind() {
        let mut shell = shell();
        let (mut value, mut ticket) = selected(&shell);
        let page = replies(message("1700000000.000001", "old"), true);
        ticket.serial += 1;
        assert!(!admit_page(&shell, &value, &ticket, &page));
        ticket.serial -= 1;
        shell.shortcut_generation += 1;
        assert!(!admit_page(&shell, &value, &ticket, &page));
        before_read(&shell, &mut value, &ticket.query);
        assert!(value.selected_thread.is_none());
        let (mut value, ticket) = selected(&shell);
        shell.core.account_epoch += 1;
        assert!(!admit_page(&shell, &value, &ticket, &page));
        before_read(&shell, &mut value, &ticket.query);
        assert!(value.selected_thread.is_none());
    }
    #[test]
    fn refused_selected_read_can_refresh_through_actual_begin_admission_without_fake_root() {
        let mut shell = shell();
        let (mut value, ticket) = selected(&shell);
        refuse(&mut value);
        assert!(value.root.is_none() && value.rows.is_empty());
        assert!(begin_replies(&shell, &mut value, "1700000000.000001"));
        assert!(value.root.is_none() && !reply_ready(&value));
        assert!(!begin_replies(&shell, &mut value, "1700000000.000009"));
        let page = replies(message("1700000000.000001", "old"), true);
        assert!(admit_page(&shell, &value, &ticket, &page));
        let wrong = replies(message("1700000000.000001", "other"), true);
        assert!(!admit_page(&shell, &value, &ticket, &wrong));
        shell.shortcut_generation += 1;
        assert!(!begin_replies(&shell, &mut value, "1700000000.000001"));
        let mut legacy = State::default();
        assert!(!begin_replies(&shell, &mut legacy, "1700000000.000001"));
        legacy.root = Some(message("1700000000.000001", "old"));
        assert!(begin_replies(&shell, &mut legacy, "1700000000.000001"));
    }
}
