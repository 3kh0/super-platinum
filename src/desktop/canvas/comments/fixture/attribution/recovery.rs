//! Closed fresh-member acquisition. Replies traverse the real known-member
//! decoder, renderer normalization and private adoption; no runtime seed replay.
use super::*;
use std::time::Instant;
use super_platinum_core::block_actions::directory_endpoint::{
    DirectoryEndpoint, member_info::Read,
};
type Reply = Result<(Value, Option<Vec<String>>), Error>;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_CANVAS_EDITOR_RECOVERY_CHECK";
pub(in crate::canvas) const LABEL: &str = "Editor Recovered";
#[derive(Clone, Copy, Default)]
pub(in crate::canvas) enum Mode {
    #[default]
    Success,
    Held,
    Offline,
    RateLimited,
}
#[derive(Default)]
struct Book {
    mode: Mode,
    requests: Vec<Value>,
    outcomes: Vec<Value>,
    held: Option<(u64, tokio::sync::oneshot::Sender<Reply>, Value)>,
    releases: Vec<Value>,
    hydration: Vec<Value>,
    stale: Vec<Value>,
    setup: Value,
    rate_started: Option<Instant>,
    expiry: Value,
}
static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
fn book() -> &'static Mutex<Book> {
    BOOK.get_or_init(Default::default)
}
pub(crate) fn requested() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-editor-recovery" | "canvas-editor-recovery-narrow")
        )
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    requested() && super::enabled(shell)
}
pub(in crate::canvas) async fn seed(state: Signal<ShellState>) -> Result<(), String> {
    if !enabled(&state.peek()) {
        return Err("Closed editor recovery owner missing".into());
    }
    let boot = data().lock().unwrap().boot["response"].clone();
    let init = json!({"ok":true,"self":boot["self"],"team":boot["team"],"workspaces":[{"id":"T1"}],
        "onboarding":{"user_data":{"onboarding_complete":true}},"user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    let flags = crate::app_home::channel_fetches::member_flags(&state.peek(), "T1")
        .map_err(|_| "Original member transformations unavailable")?;
    if !flags.current(&state.peek()) {
        return Err("Original member transformations retired".into());
    }
    book().lock().unwrap().setup = json!({"boot":boot,"init":init,"assignments":{"ok":true,"assignments":{}},"memberFlagsCurrent":true,"path":"canonical::fixture_seed"});
    Ok(())
}
fn discovery_raw() -> Value {
    json!({"ok":true,"url":"https://edgeapi.slack.com/cache/T1","ttl_seconds":300})
}
fn response() -> Value {
    let mut raw = super::member(EDITOR, LABEL, "Recovered Real");
    raw["updated"] = json!(102);
    raw["profile"]["title"] = json!("Synthetic loaded profile");
    json!({"ok":true,"results":[raw],"can_interact":{(EDITOR):true}})
}
fn exact(actual: &PreparedRequest, expected: &PreparedRequest) -> bool {
    actual.method == expected.method
        && actual.headers == expected.headers
        && policy::same_url(&actual.url, &expected.url)
        && match (&actual.body, &expected.body) {
            (RequestBody::Form(a), RequestBody::Form(b)) => a == b,
            (RequestBody::Json(a), RequestBody::Json(b)) => a == b,
            _ => false,
        }
}
fn wire_safe(request: &PreparedRequest) -> Result<Value, Error> {
    let body = match &request.body {
        RequestBody::Form(fields) => json!(
            fields
                .iter()
                .filter(|(k, _)| k != "token")
                .collect::<Vec<_>>()
        ),
        RequestBody::Json(value) => {
            let mut value = value.clone();
            let object = value.as_object_mut().ok_or_else(refused)?;
            object.remove("token");
            object.remove("enterprise_token");
            value
        }
    };
    Ok(json!({"method":request.method,"url":request.url,"body":body}))
}
pub(crate) async fn discovery(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let shell = state.peek();
    let c = carrier(&shell)?;
    if !enabled(&shell)
        || !exact(
            request,
            &DirectoryEndpoint::discovery(&shell.core.client, &c.session)?,
        )
    {
        return Err(refused());
    }
    let raw = discovery_raw();
    book()
        .lock()
        .unwrap()
        .requests
        .push(json!({"kind":"discovery","request":wire_safe(request)?,"response":raw}));
    Ok(raw)
}
struct Completion {
    serial: u64,
    done: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        if !self.done {
            book()
                .lock()
                .unwrap()
                .outcomes
                .push(json!({"serial":self.serial,"outcome":"cancelled"}));
        }
    }
}
pub(crate) async fn member(state: Signal<ShellState>, request: &PreparedRequest) -> Reply {
    let (serial, mode, raw, receive) = {
        let shell = state.peek();
        let c = carrier(&shell)?;
        let endpoint = DirectoryEndpoint::decode(&c.session, &discovery_raw(), Instant::now())?;
        let expected = endpoint.member_info(
            &shell.core.client,
            &c.session,
            Instant::now(),
            &Read::capture(vec![EDITOR.into()])?,
        )?;
        if !enabled(&shell) || !exact(request, &expected) {
            return Err(refused());
        }
        let raw = response();
        let mut b = book().lock().unwrap();
        let serial = b.requests.iter().filter(|v| v["kind"] == "member").count() as u64 + 1;
        let mode = b.mode;
        b.requests.push(json!({"kind":"member","serial":serial,"request":wire_safe(request)?,
            "response":match mode {Mode::Success|Mode::Held=>raw.clone(),_=>Value::Null},
            "mode":match mode {Mode::Success=>"success",Mode::Held=>"held",Mode::Offline=>"offline",Mode::RateLimited=>"rate-limited"}}));
        let receive = if matches!(mode, Mode::Held) {
            if b.held.is_some() {
                return Err(refused());
            }
            let (send, receive) = tokio::sync::oneshot::channel();
            b.held = Some((serial, send, raw.clone()));
            Some(receive)
        } else {
            None
        };
        (serial, mode, raw, receive)
    };
    let mut completion = Completion {
        serial,
        done: false,
    };
    let result = match (mode, receive) {
        (Mode::Held, Some(rx)) => rx.await.map_err(|_| refused())?,
        (Mode::Offline, _) => Err(Error::Offline("Synthetic editor read unavailable".into())),
        (Mode::RateLimited, _) => {
            book().lock().unwrap().rate_started = Some(Instant::now());
            Err(Error::RateLimited {
                retry_after_secs: Some(15),
            })
        }
        _ => Ok((raw, None)),
    };
    book()
        .lock()
        .unwrap()
        .outcomes
        .push(json!({"serial":serial,"outcome":match &result {
        Ok(_)=>"response",Err(Error::RateLimited{..})=>"rate-limited",Err(_)=>"offline"}}));
    completion.done = true;
    result
}
pub(in crate::canvas) fn configure(shell: &ShellState, mode: Mode) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    book().lock().unwrap().mode = mode;
    Ok(())
}
pub(in crate::canvas) fn held_closed(shell: &ShellState) -> bool {
    enabled(shell)
        && book()
            .lock()
            .unwrap()
            .held
            .as_ref()
            .is_some_and(|(_, tx, _)| tx.is_closed())
}
pub(in crate::canvas) fn release(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut b = book().lock().unwrap();
    let (serial, tx, raw) = b.held.take().ok_or_else(refused)?;
    let value = json!({"serial":serial,"delivered":tx.send(Ok((raw,None))).is_ok()});
    b.releases.push(value.clone());
    Ok(value)
}
/// Same typed-only fields as refresh_open_profile, retained after Close.
pub(in crate::canvas) fn hydrate(shell: &mut ShellState) -> Result<Value, Error> {
    if !enabled(shell) || shell.profile_user.as_deref() != Some(EDITOR) {
        return Err(refused());
    }
    let user = shell
        .core
        .workspaces
        .get_mut("T1")
        .and_then(|w| w.users.get_mut(EDITOR))
        .ok_or_else(refused)?;
    let before = serde_json::to_value(&*user).map_err(|_| refused())?;
    let mut b = book().lock().unwrap();
    let mut after = before.clone();
    after["profile"]["title"] = json!(if b.hydration.is_empty() {
        "Synthetic loaded profile"
    } else {
        "Synthetic reloaded profile"
    });
    after["im_mpim_ids"] = json!(["DATTR0001"]);
    after["has_more_mpims"] = json!(false);
    *user = serde_json::from_value(after.clone()).map_err(|_| refused())?;
    let value = json!({"before":before,"after":after,"restore":false});
    b.hydration.push(value.clone());
    Ok(value)
}
pub(in crate::canvas) fn stale(
    shell: &ShellState,
    before: Value,
    current: bool,
) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let after = json!({"profile":shell.profile_user,"navigation":shell.shortcut_generation});
    if before != after || current {
        return Err(refused());
    }
    book()
        .lock()
        .unwrap()
        .stale
        .push(json!({"before":before,"after":after,"current":current,"opened":false}));
    Ok(())
}
pub(in crate::canvas) fn snapshot(shell: &ShellState) -> Value {
    if !enabled(shell) {
        return Value::Null;
    }
    let b = book().lock().unwrap();
    json!({"setup":b.setup,"requests":b.requests,"outcomes":b.outcomes,"releases":b.releases,
        "held":b.held.as_ref().map(|(s,_,_)|s),"hydration":b.hydration,"stale":b.stale,"expiry":b.expiry,
        "native":shell.core.workspaces.get("T1").and_then(|w|w.users.get(EDITOR))})
}
pub(in crate::canvas) fn expired(shell: &ShellState) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(refused());
    }
    let mut b = book().lock().unwrap();
    let elapsed = b.rate_started.ok_or_else(refused)?.elapsed().as_millis() as u64;
    let requests = b.requests.iter().filter(|r| r["kind"] == "member").count();
    if elapsed < 15_000 || requests != 3 {
        return Err(refused());
    }
    b.expiry = json!({"elapsedMillis":elapsed,"memberRequests":requests,"automaticRequests":0});
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(id: &str) -> PreparedRequest {
        let client = super_platinum_core::slack::SlackClient::default();
        let owner = WorkspaceSession {
            team_id: "T1".into(),
            user_id: SELF.into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://canvas-attribution-fixture.slack.com".into(),
            token: TOKEN.into(),
        };
        let endpoint = DirectoryEndpoint::decode(&owner, &discovery_raw(), Instant::now()).unwrap();
        endpoint
            .member_info(
                &client,
                &owner,
                Instant::now(),
                &Read::capture(vec![id.into()]).unwrap(),
            )
            .unwrap()
    }
    #[test]
    fn original_factory_compares_exact_member_body_despite_new_correlations() {
        assert!(exact(&request(EDITOR), &request(EDITOR)));
        assert!(!exact(&request(OTHER), &request(EDITOR)));
        let mut changed = request(EDITOR);
        if let RequestBody::Json(body) = &mut changed.body {
            body["updated_ids"] = json!({EDITOR:101});
        }
        assert!(!exact(&changed, &request(EDITOR)));
    }
    #[test]
    fn fresh_response_and_evidence_preserve_selected_identity_without_credentials() {
        let raw = response();
        assert_eq!(raw["results"][0]["id"], EDITOR);
        assert_eq!(raw["results"][0]["profile"]["display_name"], LABEL);
        assert_eq!(raw["can_interact"], json!({(EDITOR):true}));
        let proof = wire_safe(&request(EDITOR)).unwrap();
        assert_eq!(proof["body"]["ids"], json!([EDITOR]));
        assert!(proof["body"].get("token").is_none());
        assert!(proof["body"].get("enterprise_token").is_none());
    }
}
