//! Exact-opt-in disconnected backend using the production request builders.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::slack::{
    SlackClient,
    client::{PreparedRequest, RequestBody},
};
pub(crate) const STAGES: &[&str] = &[
    "initial",
    "saving",
    "newest-read",
    "uncertain",
    "mismatch",
    "confirmed",
    "cooldown",
    "rotation",
    "unsupported",
    "restored",
];
#[derive(Clone, Default)]
pub(super) struct Fixture {
    pub raw: BTreeMap<String, Value>,
    pub attempts: Vec<Value>,
    pub reads: Vec<Value>,
    pub pending: Option<Attempt>,
    pub valid: bool,
}
pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_READ_PREFERENCES_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("read-preferences" | "read-preferences-narrow")
        )
}
pub(crate) fn directory() -> Result<(std::path::PathBuf, String), String> {
    if !requested() {
        return Err("Reading fixture opt-in missing".into());
    }
    let path = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_READ_PREFERENCES_DIR")
            .ok_or("Private directory missing")?,
    );
    let nonce =
        std::env::var("SUPER_PLATINUM_READ_PREFERENCES_NONCE").map_err(|_| "Nonce missing")?;
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("read-preferences-"))
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("Invalid reading fixture configuration".into());
    }
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "Private directory missing")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Invalid private directory".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Reparse directory refused".into());
        }
    }
    Ok((path, nonce))
}
pub(super) fn enabled(s: &ShellState) -> bool {
    (requested() || cfg!(test))
        && s.read_settings.fixture.is_some()
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(crate) fn install(s: &mut ShellState) {
    crate::channel_mute::fixture::install(s);
    s.core.active_account = Some("read-preferences-disconnected".into());
    s.overlay = Some(Overlay::Settings);
    s.settings_section = SettingsSection::Reading;
    let mut raw = ReadMode::LeftOffAndRead.patch();
    raw.insert("unknown_pref".into(), json!({"keep":[1,2]}));
    s.core.workspaces.get_mut("T1").unwrap().read_preferences = ReadPreferences::from_prefs(&raw);
    s.read_settings = State {
        fixture: Some(Fixture {
            raw,
            valid: true,
            ..Default::default()
        }),
        ..Default::default()
    };
}
pub(super) fn snapshot(s: &ShellState) -> ReadPreferences {
    api::parse_read_preferences(
        &json!({"ok":true,"prefs":s.read_settings.fixture.as_ref().unwrap().raw}),
    )
    .unwrap()
}
fn receipt(request: PreparedRequest) -> Value {
    let retry_safe = request.retry_safe();
    let RequestBody::Form(fields) = request.body else {
        panic!("Reading preference form required")
    };
    let url = url::Url::parse(&request.url).unwrap();
    let fields: Vec<_> = fields.into_iter().filter(|(k, _)| k != "token").collect();
    json!({"method":request.method,"url":url.as_str(),"path":url.path(),"rawQuery":url.query(),"fields":fields,"retrySafe":retry_safe})
}
fn record_read(s: &mut ShellState, token: &Token) {
    let req = api::read_preferences_request(&SlackClient::default(), &session(s, token).unwrap());
    let f = s.read_settings.fixture.as_mut().unwrap();
    let mut r = receipt(req);
    r["sequence"] = json!(f.reads.len() + f.attempts.len());
    r["response"] = json!({"ok":true,"prefs":f.raw});
    f.reads.push(r);
}
pub(super) fn read(mut state: Signal<ShellState>, r: Read) -> bool {
    let mut s = state.write();
    if !enabled(&s) {
        return false;
    }
    record_read(&mut s, &r.token);
    let fresh = snapshot(&s);
    finish_read(&mut s, &r, Ok(fresh));
    true
}
pub(super) fn dispatch(mut state: Signal<ShellState>, a: Attempt) -> bool {
    let mut s = state.write();
    if !enabled(&s) {
        return false;
    }
    record_read(&mut s, &a.token);
    let fresh = snapshot(&s);
    let req = match api::read_preferences_update_request(
        &SlackClient::default(),
        &session(&s, &a.token).unwrap(),
        &a.intent,
        &fresh,
    ) {
        Ok(req) => req,
        Err(_) => {
            rejected_fresh(&mut s, &a, &fresh);
            return true;
        }
    };
    if !preflight(&mut s, &a, &fresh) {
        return true;
    }
    let f = s.read_settings.fixture.as_mut().unwrap();
    let mut r = receipt(req);
    r["sequence"] = json!(f.reads.len() + f.attempts.len());
    f.attempts.push(r);
    f.valid &= f.pending.replace(a).is_none();
    true
}
pub(super) fn complete(s: &mut ShellState, apply: bool, result: Result<(), Error>) {
    let a = s
        .read_settings
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
        .unwrap();
    let outcome = match &result {
        Ok(()) => json!({"kind":"ack","response":{"ok":true},"applied":apply}),
        Err(Error::RateLimited { retry_after_secs }) => {
            json!({"kind":"rate-limited","status":429,"retryAfterSeconds":retry_after_secs,"applied":apply})
        }
        Err(_) => json!({"kind":"unknown","applied":apply}),
    };
    s.read_settings
        .fixture
        .as_mut()
        .unwrap()
        .attempts
        .last_mut()
        .unwrap()["outcome"] = outcome;
    if apply {
        apply_intent(s, &a.intent);
    }
    if let Some(r) = finish_write(s, &a, result) {
        record_read(s, &r.token);
        let fresh = snapshot(s);
        finish_read(s, &r, Ok(fresh));
    }
}
pub(super) fn apply_unconfirmed(s: &mut ShellState) {
    let t = Token::capture(s).unwrap();
    let intent = s.read_settings.entries[&t.owner]
        .pending
        .as_ref()
        .unwrap()
        .intent
        .clone();
    apply_intent(s, &intent);
}
fn apply_intent(s: &mut ShellState, intent: &api::ReadPreferenceIntent) {
    s.read_settings
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .extend(intent.desired().patch());
}
