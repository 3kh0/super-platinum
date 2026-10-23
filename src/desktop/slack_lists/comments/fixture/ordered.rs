//! Original authored HTTP bytes for one closed rich-edit prerequisite.
use super::*;
use super_platinum_core::slack_lists::comments::ordered::ResponseWitness;

const INITIAL: &str = include_str!("ordered/initial.json");
const FUTURE: &str = include_str!("ordered/future.json");
#[derive(Default)]
pub(super) struct State {
    requests: Vec<Value>,
    pages: Vec<Value>,
    releases: Vec<Value>,
    held: Option<oneshot::Sender<()>>,
    hold_next: bool,
    future: bool,
    completed: usize,
    cancelled: usize,
    original_held: Option<(Owner, u64)>,
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_ORDERED_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-ordered")
}
fn enabled_case() -> bool {
    named() || super::plain::named()
}
fn initial_bytes() -> &'static str {
    if super::plain::emoji_display::named() {
        super::plain::emoji_display::INITIAL
    } else {
        INITIAL
    }
}
fn future_bytes() -> &'static str {
    if super::plain::named() {
        super::plain::future()
    } else {
        FUTURE
    }
}
fn decoded(bytes: &str) -> Result<ResponseWitness, Error> {
    let raw: Value = serde_json::from_str(bytes).map_err(|_| unavailable())?;
    ResponseWitness::decode(bytes.as_bytes(), &raw).map_err(|_| unavailable())
}
pub(crate) fn initial_message() -> Value {
    decoded(initial_bytes())
        .expect("authored initial replies")
        .response()["messages"][1]
        .clone()
}
pub(crate) fn future_message() -> Value {
    decoded(future_bytes())
        .expect("authored future replies")
        .response()["messages"][1]
        .clone()
}
fn original(s: &ShellState) -> Result<Model, Error> {
    if !enabled_case() || !own_edit::enabled(s) {
        return Err(unavailable());
    }
    s.slack_lists
        .comments
        .fixture
        .clone()
        .ok_or_else(unavailable)
}
struct Reading {
    model: Model,
    accepted: bool,
}
impl Drop for Reading {
    fn drop(&mut self) {
        if let Ok(mut l) = self.model.0.lock() {
            l.completed += 1;
            l.ordered.completed += 1;
            if !self.accepted {
                l.cancelled += 1;
                l.ordered.cancelled += 1;
            }
        }
    }
}
pub(in crate::slack_lists::comments) async fn read(
    state: Signal<ShellState>,
    owner: &Owner,
    read_serial: u64,
    request: &PreparedRequest,
) -> Option<Result<(usize, ResponseWitness), Error>> {
    if !enabled_case() {
        return None;
    }
    Some(read_owned(state, owner, read_serial, request).await)
}
async fn read_owned(
    state: Signal<ShellState>,
    owner: &Owner,
    read_serial: u64,
    request: &PreparedRequest,
) -> Result<(usize, ResponseWitness), Error> {
    let model = original(&state.peek())?;
    if !owner.reading(&state.peek(), read_serial) {
        return Err(unavailable());
    }
    let (method, args) = fields(request)?;
    if method != "conversations.replies"
        || args.get("ts").map(String::as_str) != Some(ROOT)
        || args.contains_key("cursor")
    {
        return Err(unavailable());
    }
    let (serial, bytes, wait) = {
        let s = state.peek();
        let mut l = model.0.lock().map_err(|_| unavailable())?;
        let serial = l.requests.len() + 1;
        let bytes = if l.ordered.future {
            future_bytes()
        } else {
            initial_bytes()
        };
        let response = decoded(bytes)?;
        let variant = if l.ordered.future {
            "future"
        } else {
            "initial"
        };
        l.requests
            .push(json!({"serial":serial,"method":method,"args":args}));
        l.replies += 1;
        l.ordered.requests.push(json!({"serial":serial,"readSerial":read_serial,"method":method,"args":args,"variant":variant,"bytes":bytes,"raw":response.response(),"owner":{"serial":owner.serial,"account":s.core.active_account,"epoch":s.core.account_epoch}}));
        let wait = if std::mem::take(&mut l.ordered.hold_next) {
            l.ordered.original_held = Some((owner.clone(), read_serial));
            let (tx, rx) = oneshot::channel();
            l.ordered.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (serial, bytes, wait)
    };
    let mut pending = Reading {
        model,
        accepted: false,
    };
    if let Some(wait) = wait {
        let retired = async {
            loop {
                if !owner.reading(&state.peek(), read_serial) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        };
        tokio::select! { biased; _=retired=>return Err(unavailable()), r=wait=>r.map_err(|_|unavailable())? }
    }
    if !owner.reading(&state.peek(), read_serial) {
        return Err(unavailable());
    }
    let response = decoded(bytes)?;
    pending.accepted = true;
    Ok((serial, response))
}
fn page_proof(page: &Page) -> Result<Value, Error> {
    let w = page.ordered_response().ok_or_else(unavailable)?;
    let m = w.message(1).map_err(|_| unavailable())?;
    Ok(
        json!({"raw":page.raw(),"messages":page.messages(),"witness":{"messageIndex":1,"raw":m.raw_message(),"styleKeys":m.style_keys(&[0,0,0])}}),
    )
}
pub(in crate::slack_lists::comments) fn observe_page(
    state: Signal<ShellState>,
    owner: &Owner,
    read_serial: u64,
    request_serial: usize,
    page: &Page,
) -> Result<(), Error> {
    let model = original(&state.peek())?;
    if !owner.reading(&state.peek(), read_serial) {
        return Err(unavailable());
    }
    let mut proof = page_proof(page)?;
    let mut l = model.0.lock().map_err(|_| unavailable())?;
    let r = l
        .ordered
        .requests
        .iter()
        .find(|r| r["serial"] == request_serial)
        .ok_or_else(unavailable)?;
    if r["readSerial"] != read_serial
        || r["owner"]["serial"] != owner.serial
        || r["raw"] != *page.raw()
        || l.ordered
            .pages
            .iter()
            .any(|p| p["requestSerial"] == request_serial)
    {
        return Err(unavailable());
    }
    proof["requestSerial"] = json!(request_serial);
    proof["readSerial"] = json!(read_serial);
    proof["current"] = json!(true);
    l.ordered.pages.push(proof);
    Ok(())
}
pub(crate) fn publish(s: &ShellState) -> Result<(), String> {
    let actual = own_edit::snapshot(s)["responses"][0]["raw"]["message"].clone();
    if actual != future_message() {
        return Err(format!(
            "Actual changed ACK differs from independently authored future message: {actual}"
        ));
    }
    let m = original(s).map_err(|e| e.to_string())?;
    m.0.lock()
        .map_err(|_| "Ordered ledger poisoned")?
        .ordered
        .future = true;
    Ok(())
}
pub(crate) fn hold(s: &ShellState) -> Result<(), String> {
    let m = original(s).map_err(|e| e.to_string())?;
    let mut l = m.0.lock().map_err(|_| "Ordered ledger poisoned")?;
    if l.ordered.held.is_some() || l.ordered.hold_next {
        return Err("Original ordered read already held".into());
    }
    l.ordered.hold_next = true;
    Ok(())
}
pub(crate) fn release(m: &Model) -> Result<Value, String> {
    let mut l = m.0.lock().map_err(|_| "Ordered ledger poisoned")?;
    let tx = l
        .ordered
        .held
        .take()
        .ok_or("No held original ordered reply")?;
    let cancelled = tx.is_closed();
    let delivered = tx.send(()).is_ok();
    let p = json!({"cancelledBeforeReply":cancelled,"delivered":delivered});
    l.ordered.releases.push(p.clone());
    Ok(p)
}
pub(crate) fn snapshot(s: &ShellState, m: &Model) -> Value {
    let l = m.0.lock().unwrap();
    let o = &l.ordered;
    json!({"initialBytes":initial_bytes(),"futureBytes":future_bytes(),"requests":o.requests,"pages":o.pages,"releases":o.releases,"held":o.held.is_some(),"completed":o.completed,"cancelled":o.cancelled,"heldOwnerCurrent":o.original_held.as_ref().map(|(owner,read)|owner.reading(s,*read)),"entryPage":s.slack_lists.comments.entry.as_ref().and_then(|e|e.page.as_ref()).map(page_proof).transpose().ok().flatten(),"plainModeEnabled":super::plain::named()})
}

/// Capture only the original private database address, before owner replacement.
#[derive(Clone)]
pub(crate) struct Disk {
    path: std::path::PathBuf,
    account: String,
    scope: super_platinum_core::slack_lists::comments::journal::Scope,
}
impl Disk {
    pub(crate) fn capture(s: &ShellState) -> Result<Self, String> {
        original(s).map_err(|e| e.to_string())?;
        let session = compose::session(s).map_err(|e| e.to_string())?;
        let entry = s
            .slack_lists
            .comments
            .entry
            .as_ref()
            .ok_or("Original comments missing")?;
        Ok(Self {
            path: compose::path(s).ok_or("Original private database missing")?,
            account: s
                .core
                .active_account
                .clone()
                .ok_or("Original account missing")?,
            scope: super_platinum_core::slack_lists::comments::journal::Scope::capture(
                &session.team_id,
                &session.user_id,
                &entry.target,
            )
            .map_err(|e| e.to_string())?,
        })
    }
    pub(crate) async fn snapshot(&self) -> Result<Value, String> {
        let own = self.clone();
        tokio::task::spawn_blocking(move || {
            let j=super_platinum_core::slack_lists::comments::journal::Journal::open(&own.path,&own.account).map_err(|e|e.to_string())?;
            Ok(json!({"path":own.path,"account":own.account,"scope":own.scope,"reply":j.inventory(&own.scope).map_err(|e|e.to_string())?,"edit":j.edit_inventory(&own.scope,own_edit::RECEIVED_TS).map_err(|e|e.to_string())?}))
        }).await.map_err(|e|e.to_string())?
    }
}
