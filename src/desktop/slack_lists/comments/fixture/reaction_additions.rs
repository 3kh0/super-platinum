//! Named closed cold-preference replies. No session/Transport is installed.
use super::*;
pub(crate) const FIRST: &str = "1781524801.000001";
pub(crate) const RAW: [&str; 6] = [
    "wave",
    "wave::skin-tone-3",
    "handshake",
    "handshake::skin-tone-2-6",
    "thumbsup",
    "+1",
];
pub(crate) const RESOLVED: [&str; 6] = [
    "wave::skin-tone-4",
    "wave::skin-tone-3",
    "handshake::skin-tone-4",
    "handshake::skin-tone-2-6",
    "thumbsup::skin-tone-4",
    "+1::skin-tone-4",
];
#[derive(Clone, Copy, Debug, Default)]
pub(crate) enum PrefMode {
    #[default]
    Held,
    Success,
    Failure,
}
#[derive(Default)]
pub(super) struct Preferences {
    mode: PrefMode,
    requests: Vec<Value>,
    outcomes: Vec<Value>,
    held: Option<oneshot::Sender<Value>>,
    completed: u64,
    cancelled: u64,
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    cfg!(feature = "protected-native-fixture")
        && compose::enabled(s)
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("pill-additions" | "pill-additions-custody")
        )
}
pub(crate) fn same_model(a: &Model, b: &Model) -> bool {
    Arc::ptr_eq(&a.0, &b.0)
}
pub(crate) fn model(s: &ShellState) -> Option<Model> {
    enabled(s)
        .then(|| s.slack_lists.comments.fixture.clone())
        .flatten()
}
pub(crate) fn owns(s: &ShellState, m: &Model) -> bool {
    enabled(s)
        && s.slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|v| same_model(v, m))
}
pub(crate) fn allowed(method: &str, name: &str) -> bool {
    method == "reactions.add" && RESOLVED.contains(&name)
}
pub(super) fn seed(s: &ShellState) -> Result<(), String> {
    if !enabled(s)
        || s.core.workspaces["T1"].emoji_preferences.loaded()
        || crate::emoji_catalog::current(s).is_some()
    {
        return Err("Pill additions must begin with genuinely cold preferences/catalog".into());
    }
    Ok(())
}
pub(crate) fn initial_reactions() -> Value {
    json!(RAW.map(|name| json!({"name":name,"count":1,"users":["U1"]})))
}
pub(super) fn decorate(raw: &mut Value) {
    if let Some(row) = raw["messages"]
        .as_array_mut()
        .and_then(|a| a.iter_mut().find(|r| r["ts"] == FIRST))
    {
        row["reactions"] = initial_reactions();
        row["pill_additions_opaque"] =
            json!({"keep":"original raw names and independent membership"});
    }
}
pub(crate) fn preference_response() -> Value {
    json!({"ok":true,"prefs":{"preferred_skin_tone":4,"emoji_mode":"native","emoji_use":{},"frecency":"{}"},"fixture_opaque":"original cold preference response"})
}
pub(crate) fn preference_mode(s: &ShellState, m: &Model, mode: PrefMode) -> Result<(), String> {
    if !owns(s, m) {
        return Err("Original closed preference owner required".into());
    }
    let mut l = m.0.lock().map_err(|_| "Preference ledger poisoned")?;
    if l.additions.held.is_some() {
        return Err("Release original preferences first".into());
    }
    l.additions.mode = mode;
    Ok(())
}
pub(crate) fn release_preferences(m: &Model) -> Result<Value, String> {
    let tx =
        m.0.lock()
            .map_err(|_| "Preference ledger poisoned")?
            .additions
            .held
            .take()
            .ok_or("No original preferences held")?;
    let cancelled = tx.is_closed();
    let raw = preference_response();
    let delivered = tx.send(raw.clone()).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":cancelled,"raw":raw}))
}
fn validate_preferences(
    request: &PreparedRequest,
    client: &super_platinum_core::slack::SlackClient,
    session: &WorkspaceSession,
) -> Result<(), Error> {
    let expected = super_platinum_core::composer_emoji::api::preferences_request(client, session);
    let url = url::Url::parse(&request.url).map_err(|_| unavailable())?;
    let exact = url::Url::parse(&expected.url).map_err(|_| unavailable())?;
    if request.method != expected.method
        || request.headers != expected.headers
        || request.body != expected.body
        || url.scheme() != exact.scheme()
        || url.host_str() != exact.host_str()
        || url.path() != exact.path()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !reaction_actions::fixture_query(&url)
    {
        return Err(unavailable());
    }
    Ok(())
}
struct Pending(Model, bool);
impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut l) = self.0.0.lock() {
            l.additions.completed += 1;
            if !self.1 {
                l.additions.cancelled += 1;
            }
        }
    }
}
pub(crate) async fn preferences_request(
    state: Signal<ShellState>,
    m: &Model,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let session = {
        let s = state.peek();
        if !owns(&s, m) {
            return Err(unavailable());
        }
        compose::session(&s)?
    };
    validate_preferences(request, &state.peek().core.client, &session)?;
    let (mode, rx) = {
        let mut l = m.0.lock().map_err(|_| unavailable())?;
        let mode = l.additions.mode;
        l.additions.requests.push(json!({"method":"users.prefs.get","args":{"prefs":"preferred_skin_tone,emoji_mode,emoji_use,frecency"},"mode":format!("{mode:?}")}));
        let rx = if matches!(mode, PrefMode::Held) {
            let (tx, rx) = oneshot::channel();
            l.additions.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (mode, rx)
    };
    let mut pending = Pending(m.clone(), false);
    let result = match (mode, rx) {
        (PrefMode::Held, Some(rx)) => rx.await.map_err(|_| unavailable()),
        (PrefMode::Failure, _) => Err(Error::Offline(
            "Closed preference fixture is offline".into(),
        )),
        _ => Ok(preference_response()),
    };
    pending.1 = true;
    if let Ok(mut l) = m.0.lock() {
        l.additions
            .outcomes
            .push(json!({"kind":"response","raw":result.as_ref().ok(),"failed":result.is_err()}));
    }
    result
}
pub(crate) fn preference_outcome(s: &ShellState, m: &Model, error: Option<Error>) {
    if !owns(s, m) {
        return;
    }
    if let Ok(mut l) = m.0.lock() {
        l.additions.outcomes.push(json!({"kind":"adoption","failed":error.is_some(),"preferences":s.core.workspaces["T1"].emoji_preferences}));
    }
}
pub(crate) fn observed(s: &ShellState, m: &Model) -> Result<Value, String> {
    if !owns(s, m) {
        return Err("Original additions owner required".into());
    }
    let l = m.0.lock().map_err(|_| "Preference ledger poisoned")?;
    Ok(
        json!({"preferences":s.core.workspaces["T1"].emoji_preferences,"customRevision":s.core.workspaces["T1"].custom_emoji_revision,
        "catalog":crate::emoji_catalog::current(s).map(|c|json!({"token":c.token,"current":c.current(s)})),
        "requests":l.additions.requests,"outcomes":l.additions.outcomes,"held":l.additions.held.is_some(),"completed":l.additions.completed,"cancelled":l.additions.cancelled}),
    )
}
pub(crate) fn server(s: &ShellState, m: &Model, reactions: Value) -> Result<Value, String> {
    if !owns(s, m) || !reactions.is_array() {
        return Err("Original future source required".into());
    }
    let mut l = m.0.lock().map_err(|_| "Preference ledger poisoned")?;
    l.reaction_readbacks.insert(FIRST.into(), reactions);
    let mut raw = reply(ROOT, false);
    decorate(&mut raw);
    apply_reaction_readbacks(&l.reaction_readbacks, &mut raw);
    Ok(raw)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_factory_is_exact_and_does_not_admit_writes_or_extra_fields() {
        let client = super_platinum_core::slack::SlackClient::default();
        let session = super::super::session();
        let request =
            super_platinum_core::composer_emoji::api::preferences_request(&client, &session);
        assert!(validate_preferences(&request, &client, &session).is_ok());
        let mut extra = request.clone();
        if let RequestBody::Form(ref mut fields) = extra.body {
            fields.push(("other".into(), "1".into()));
        }
        assert!(validate_preferences(&extra, &client, &session).is_err());
        let mut query = request.clone();
        query.url.push_str("&unobserved=true");
        assert!(validate_preferences(&query, &client, &session).is_err());
        let mut wrong = request;
        wrong.url = wrong.url.replace("users.prefs.get", "users.prefs.set");
        assert!(validate_preferences(&wrong, &client, &session).is_err());
    }
    #[test]
    fn original_complete_reply_parses_preferred_four_and_raw_names_stay_distinct() {
        let raw = preference_response();
        let parsed = super_platinum_core::composer_emoji::api::parse_preferences(&raw).unwrap();
        let mut prefs = super_platinum_core::composer_emoji::preferences::Preferences::default();
        assert!(!prefs.loaded());
        assert!(prefs.replace(&parsed));
        assert_eq!(prefs.preferred_tone(), Some("4"));
        assert_eq!(RAW[4], "thumbsup");
        assert_eq!(RAW[5], "+1");
        assert_ne!(RESOLVED[4], RESOLVED[5]);
        for name in RESOLVED {
            assert!(allowed("reactions.add", name));
            assert!(!allowed("reactions.remove", name));
        }
        assert!(!allowed("chat.postMessage", "wave"));
    }
}
