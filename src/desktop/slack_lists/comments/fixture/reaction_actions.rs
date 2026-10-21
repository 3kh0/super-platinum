//! Closed existing-pill dispatcher with immutable request and reply receipts.
use super::*;
use super_platinum_core::slack::{api, transport::ListCommentReactionFailure as Failure};
const TS: &str = "1781524801.000001";
const CHANNEL: &str = "CCOMMENT01";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Success,
    Held,
    Rejected,
    Offline,
    Unconfirmed,
    Malformed,
    RateLimited,
}
#[derive(Default)]
pub(super) struct Ledger {
    mode: Mode,
    modes: std::collections::VecDeque<Mode>,
    prepared: Vec<Value>,
    issued: Vec<Value>,
    outcomes: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
    completed: u64,
    full_reads: Vec<Value>,
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    compose::enabled(s)
        && (reaction_additions::enabled(s)
            || matches!(
                std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
                Ok("reaction-actions"
                    | "reaction-failures"
                    | "reaction-family"
                    | "reaction-family-failures"
                    | "reaction-selection"
                    | "reaction-selection-failures"
                    | "reaction-selection-custody")
            ))
}
pub(crate) fn mode(s: &ShellState, model: &Model, mode: Mode) -> Result<(), String> {
    if !enabled(s)
        || !s
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
    {
        return Err("Original closed reaction fixture required".into());
    }
    let mut l = model.0.lock().map_err(|_| "Reaction ledger poisoned")?;
    if l.actions.held.is_some() {
        return Err("Release original reaction first".into());
    }
    l.actions.mode = mode;
    Ok(())
}
// rest_form always adds the fixture client's exact XParams. Only its three
// generated trace values differ between equivalent factory invocations.
pub(super) fn fixture_query(url: &url::Url) -> bool {
    let expected = super_platinum_core::slack::xparams::XParams::default()
        .rest_pairs()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let mut fields = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if fields
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return false;
        }
    }
    fields.len() == expected.len()
        && expected.iter().all(|(key, expected)| {
            fields.get(key).is_some_and(|value| {
                if matches!(key.as_str(), "_x_id" | "_x_csid" | "fp") {
                    value.len() == 32
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                } else {
                    value == expected
                }
            })
        })
}
fn wire(request: &PreparedRequest) -> Result<Value, Error> {
    wire_for(request, false)
}
fn wire_for(request: &PreparedRequest, family: bool) -> Result<Value, Error> {
    wire_scope(request, family, false, false)
}
fn wire_scope(
    request: &PreparedRequest,
    family: bool,
    selection: bool,
    additions: bool,
) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| unavailable())?;
    if request.method != "POST"
        || url.scheme() != "https"
        || url.host_str() != Some("example.test")
        || !url.username().is_empty()
        || url.password().is_some()
        || !fixture_query(&url)
        || url.fragment().is_some()
    {
        return Err(unavailable());
    }
    let method = url.path().strip_prefix("/api/").ok_or_else(unavailable)?;
    if !["reactions.add", "reactions.remove"].contains(&method) {
        return Err(unavailable());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(unavailable());
    };
    let mut args = BTreeMap::new();
    for (k, v) in fields {
        if args.insert(k.clone(), v.clone()).is_some() {
            return Err(unavailable());
        }
    }
    let name = args.get("name").cloned().ok_or_else(unavailable)?;
    if (family && (method != "reactions.remove" || !reaction_family::OWN.contains(&name.as_str())))
        || (selection && !reaction_selection::allowed(method, &name))
        || (additions && !reaction_additions::allowed(method, &name))
        || (!family && !selection && !additions && name != "wave")
        || args.remove("token").as_deref() != Some("fixture-not-a-token")
        || args
            != BTreeMap::from([
                ("channel".into(), CHANNEL.into()),
                ("timestamp".into(), TS.into()),
                ("name".into(), name),
            ])
    {
        return Err(unavailable());
    }
    Ok(json!({"method":method,"args":args}))
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    id: u64,
    request: &PreparedRequest,
) -> Result<Value, Failure> {
    let (model, form, user) = {
        let shell = state.peek();
        if !enabled(&shell) {
            return Err(Failure::NotRequested(unavailable()));
        }
        let session = compose::session(&shell).map_err(Failure::NotRequested)?;
        let family = reaction_family::enabled(&shell);
        let selection = reaction_selection::enabled(&shell);
        let additions = reaction_additions::enabled(&shell);
        let form =
            wire_scope(request, family, selection, additions).map_err(Failure::NotRequested)?;
        let name = form["args"]["name"]
            .as_str()
            .ok_or_else(|| Failure::NotRequested(unavailable()))?;
        let exact = if form["method"] == "reactions.add" {
            api::reactions_add(
                &shell.core.client,
                &session,
                CHANNEL.into(),
                TS.into(),
                name.into(),
            )
        } else {
            api::reactions_remove(
                &shell.core.client,
                &session,
                CHANNEL.into(),
                TS.into(),
                name.into(),
            )
        };
        if request.headers != exact.headers
            || wire_scope(&exact, family, selection, additions).map_err(Failure::NotRequested)?
                != form
        {
            return Err(Failure::NotRequested(unavailable()));
        }
        (
            shell.slack_lists.comments.fixture.clone().unwrap(),
            form,
            session.user_id,
        )
    };
    let (mode, raw, wait) = {
        let mut l = model
            .0
            .lock()
            .map_err(|_| Failure::NotRequested(unavailable()))?;
        let l = &mut l.actions;
        let mode = l
            .modes
            .pop_front()
            .unwrap_or_else(|| std::mem::take(&mut l.mode));
        let prepared = json!({"id":id,"request":form,"user":user});
        l.prepared.push(prepared.clone());
        if mode != Mode::Offline {
            l.issued.push(prepared);
        }
        let raw = match mode {
            Mode::Rejected => json!({"ok":false,"error":"invalid_name"}),
            Mode::Malformed => json!({"ok":"true","opaque":"malformed-reaction-receipt"}),
            _ => json!({"ok":true}),
        };
        let wait = if mode == Mode::Held {
            let (tx, rx) = oneshot::channel();
            l.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (mode, raw, wait)
    };
    if let Some(rx) = wait {
        rx.await.map_err(|_| Failure::Unconfirmed(unavailable()))?;
    }
    {
        let mut l = model
            .0
            .lock()
            .map_err(|_| Failure::Unconfirmed(unavailable()))?;
        l.actions.completed += 1;
        let mut outcome = json!({"id":id,"disposition":match mode{Mode::Offline=>"not-requested",Mode::Unconfirmed|Mode::RateLimited=>"unconfirmed",_=>"response"},"raw":if matches!(mode,Mode::Offline|Mode::Unconfirmed|Mode::RateLimited){Value::Null}else{raw.clone()}});
        if mode == Mode::RateLimited {
            outcome["failure"] = json!("rate-limited");
            outcome["retryAfterSecs"] = json!(2);
        }
        l.actions.outcomes.push(outcome);
    }
    match mode {
        Mode::Offline => Err(Failure::NotRequested(Error::Offline(
            "Synthetic reaction link unavailable".into(),
        ))),
        Mode::Unconfirmed => Err(Failure::Unconfirmed(Error::Transport(
            "Synthetic reaction response lost".into(),
        ))),
        Mode::RateLimited => Err(Failure::Unconfirmed(Error::RateLimited {
            retry_after_secs: Some(2),
        })),
        _ => Ok(raw),
    }
}
pub(crate) fn sequence(s: &ShellState, model: &Model, modes: &[Mode]) -> Result<(), String> {
    if !reaction_family::enabled(s)
        || !s
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
    {
        return Err("Original closed family fixture required".into());
    }
    let mut l = model.0.lock().map_err(|_| "Reaction ledger poisoned")?;
    if l.actions.held.is_some() || !l.actions.modes.is_empty() || modes.is_empty() {
        return Err("Previous family requests must settle".into());
    }
    l.actions.modes = modes.iter().copied().collect();
    Ok(())
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let tx = model
        .0
        .lock()
        .map_err(|_| "Reaction ledger poisoned")?
        .actions
        .held
        .take()
        .ok_or("No held reaction")?;
    let cancelled = tx.is_closed();
    let delivered = tx.send(()).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":cancelled}))
}
pub(super) fn snapshot(l: &Ledger) -> Value {
    json!({"prepared":l.prepared,"issued":l.issued,"outcomes":l.outcomes,"completed":l.completed,"held":l.held.is_some(),"networkCalls":0,"fullReads":l.full_reads})
}
/// Separately record what a subsequent canonical server read will return.
pub(crate) fn server(state: &ShellState, model: &Model, own: bool) -> Result<Value, String> {
    let user = compose::session(state)
        .map_err(|_| "Reaction self missing")?
        .user_id;
    let mut users = vec!["U1".to_owned(), "U2".to_owned()];
    if own {
        users.push(user);
    }
    server_members(state, model, users)
}
pub(crate) fn server_members(
    state: &ShellState,
    model: &Model,
    users: Vec<String>,
) -> Result<Value, String> {
    if !enabled(state)
        || !state
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
    {
        return Err("Original reaction server required".into());
    }
    let user = compose::session(state)
        .map_err(|_| "Reaction self missing")?
        .user_id;
    let unique = users.iter().collect::<std::collections::BTreeSet<_>>();
    if unique.len() != users.len() || users.iter().any(|u| u != "U1" && u != "U2" && u != &user) {
        return Err("Foreign reaction server membership".into());
    }
    let own = users.contains(&user);
    let reactions = if users.is_empty() {
        json!([])
    } else {
        json!([{"name":"wave","count":users.len(),"users":users}])
    };
    let mut l = model.0.lock().map_err(|_| "Reaction ledger poisoned")?;
    l.reaction_readbacks.insert(TS.into(), reactions.clone());
    let mut raw = reply(ROOT, false);
    apply_reaction_readbacks(&l.reaction_readbacks, &mut raw);
    Ok(json!({"ts":TS,"own":own,"reactions":reactions,"response":raw}))
}
pub(crate) async fn get(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = {
        let shell = state.peek();
        if !enabled(&shell) {
            return Err(unavailable());
        }
        shell
            .slack_lists
            .comments
            .fixture
            .clone()
            .ok_or_else(unavailable)?
    };
    let form = full_wire(request)?;
    let mut l = model.0.lock().map_err(|_| unavailable())?;
    let mut page = reply(ROOT, false);
    if reaction_family::enabled(&state.peek()) {
        reaction_family::decorate(&mut page);
    }
    apply_reaction_readbacks(&l.reaction_readbacks, &mut page);
    let response = json!({"ok":true,"type":"message","channel":CHANNEL,"message":{"ts":TS,"thread_ts":ROOT,"reactions":page["messages"][1]["reactions"]}});
    l.actions
        .full_reads
        .push(json!({"request":form,"response":response}));
    Ok(response)
}
fn full_wire(request: &PreparedRequest) -> Result<Value, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| unavailable())?;
    if request.method != "POST"
        || url.origin().ascii_serialization() != "https://example.test"
        || url.path() != "/api/reactions.get"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !fixture_query(&url)
    {
        return Err(unavailable());
    }
    let RequestBody::Form(fields) = &request.body else {
        return Err(unavailable());
    };
    let mut args = BTreeMap::new();
    for (k, v) in fields {
        if args.insert(k.clone(), v.clone()).is_some() {
            return Err(unavailable());
        }
    }
    if args.remove("token").as_deref() != Some("fixture-not-a-token")
        || args
            != BTreeMap::from([
                ("channel".into(), CHANNEL.into()),
                ("timestamp".into(), TS.into()),
                ("full".into(), "true".into()),
            ])
    {
        return Err(unavailable());
    }
    Ok(json!({"method":"reactions.get","args":args}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_wire_retains_exact_factory_and_rejects_sibling_removal() {
        let client = super_platinum_core::slack::client::SlackClient::default();
        for factory in [api::reactions_add, api::reactions_remove] {
            for name in [reaction_selection::TONE, reaction_selection::CUSTOM, "eyes"] {
                let mut request =
                    factory(&client, &session(), CHANNEL.into(), TS.into(), name.into());
                assert_eq!(
                    wire_scope(&request, false, true, false).unwrap()["args"]["name"],
                    name
                );
                assert!(wire(&request).is_err());
                let RequestBody::Form(fields) = &mut request.body else {
                    panic!()
                };
                fields.push(("family".into(), "+1".into()));
                assert!(wire_scope(&request, false, true, false).is_err());
            }
        }
        let request = api::reactions_remove(
            &client,
            &session(),
            CHANNEL.into(),
            TS.into(),
            "+1::skin-tone-2".into(),
        );
        assert!(wire_scope(&request, false, true, false).is_err());
    }
    #[test]
    fn family_closed_wire_accepts_only_exact_planned_removal_names() {
        let client = super_platinum_core::slack::client::SlackClient::default();
        for name in reaction_family::OWN {
            let request = api::reactions_remove(
                &client,
                &session(),
                CHANNEL.into(),
                TS.into(),
                (*name).into(),
            );
            assert_eq!(wire_for(&request, true).unwrap()["args"]["name"], *name);
            assert!(wire(&request).is_err());
            let add = api::reactions_add(
                &client,
                &session(),
                CHANNEL.into(),
                TS.into(),
                (*name).into(),
            );
            assert!(wire_for(&add, true).is_err());
        }
        for name in [
            reaction_family::SELECTED,
            "+1::skin-tone-3",
            "wave",
            "thumbsup::skin-tone-9",
        ] {
            let request =
                api::reactions_remove(&client, &session(), CHANNEL.into(), TS.into(), name.into());
            assert!(wire_for(&request, true).is_err());
        }
        let mut request = api::reactions_remove(
            &client,
            &session(),
            CHANNEL.into(),
            TS.into(),
            "thumbsup".into(),
        );
        let RequestBody::Form(fields) = &mut request.body else {
            panic!()
        };
        fields.push(("family".into(), "thumbsup".into()));
        assert!(wire_for(&request, true).is_err());
    }
    #[test]
    fn default_factory_trace_query_is_required_and_cannot_hide_extra_arguments() {
        let client = super_platinum_core::slack::client::SlackClient::default();
        for endpoint in ["reactions.add", "reactions.get"] {
            let original = if endpoint == "reactions.add" {
                api::reactions_add(
                    &client,
                    &session(),
                    CHANNEL.into(),
                    TS.into(),
                    "wave".into(),
                )
            } else {
                api::reactions_get(&client, &session(), CHANNEL.into(), TS.into())
            };
            let decode = if endpoint == "reactions.add" {
                wire
            } else {
                full_wire
            };
            assert!(decode(&original).is_ok());
            for addition in [
                "channel=COTHER",
                "_x_num_retries=0",
                "token=fixture-not-a-token",
            ] {
                let mut altered = original.clone();
                altered.url.push('&');
                altered.url.push_str(addition);
                assert!(decode(&altered).is_err());
            }
            let mut altered = original.clone();
            altered.url = original.url.split('?').next().unwrap().to_owned();
            assert!(decode(&altered).is_err());
            let mut altered = original.clone();
            let mut url = url::Url::parse(&altered.url).unwrap();
            let pairs = url
                .query_pairs()
                .map(|(k, v)| {
                    (
                        k.into_owned(),
                        if v == "0" { "1".into() } else { v.into_owned() },
                    )
                })
                .collect::<Vec<_>>();
            url.set_query(None);
            url.query_pairs_mut().extend_pairs(pairs);
            altered.url = url.to_string();
            assert!(decode(&altered).is_err());
        }
    }
    #[test]
    fn full_reaction_read_requires_exact_full_factory_without_paging() {
        let client = super_platinum_core::slack::client::SlackClient::default();
        let mut request = api::reactions_get(&client, &session(), CHANNEL.into(), TS.into());
        assert_eq!(
            full_wire(&request).unwrap(),
            json!({"method":"reactions.get","args":{"channel":CHANNEL,"timestamp":TS,"full":"true"}})
        );
        let RequestBody::Form(fields) = &mut request.body else {
            panic!()
        };
        fields.iter_mut().find(|(k, _)| k == "full").unwrap().1 = "false".into();
        assert!(full_wire(&request).is_err());
    }
    #[test]
    fn closed_reaction_wire_uses_exact_factories_and_refuses_extra_target_fields() {
        let client = super_platinum_core::slack::client::SlackClient::default();
        let session = session();
        for add in [true, false] {
            let mut request = if add {
                api::reactions_add(&client, &session, CHANNEL.into(), TS.into(), "wave".into())
            } else {
                api::reactions_remove(&client, &session, CHANNEL.into(), TS.into(), "wave".into())
            };
            assert_eq!(
                wire(&request).unwrap()["method"],
                if add {
                    "reactions.add"
                } else {
                    "reactions.remove"
                }
            );
            let RequestBody::Form(fields) = &mut request.body else {
                panic!()
            };
            fields.push(("file".into(), LIST.into()));
            assert!(wire(&request).is_err());
        }
        for (channel, ts, name) in [
            ("COTHER", TS, "wave"),
            (CHANNEL, "1781524999.000001", "wave"),
            (CHANNEL, TS, "smile"),
        ] {
            assert!(
                wire(&api::reactions_add(
                    &client,
                    &session,
                    channel.into(),
                    ts.into(),
                    name.into()
                ))
                .is_err()
            );
        }
    }
}
