//! Closed post-ingress revision injection and held body responses for the real effect.
//! No scheduler, claim, finish, transport, or canonical assignment lives here.
use super::{return_body as body, *};
use crate::canvas::checklist::{self, scope::Scope};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::time::Instant;
use super_platinum_core::canvas::wire::LoadDataResponse;
use super_platinum_core::{
    canvas::CanvasDocument,
    slack::{
        api::{CanvasCollabSnapshot, CanvasControllerInit},
        models::File,
    },
};
const FILE: &str = "Ffixture";
pub(in crate::canvas) const TITLE: &str = "Latest Project handbook";
pub(in crate::canvas) const CONTENT: &str =
    "Latest automatic body. Original rich reply stays saved.";
const OLD_TITLE: &str = "Superseded Project handbook";
const OLD_BODY: &str = "This older automatic body must never replace the current view.";
const HTML: &str = "<div class='quip-canvas-content'><h1>Latest Project handbook</h1><p>Latest automatic body. Original rich reply stays saved.</p><p>Read-only acquired body &amp; \u{4e2d}\u{1f98a}</p></div>";
const OLD_HTML: &str = "<div class='quip-canvas-content'><h1>Superseded Project handbook</h1><p>This older automatic body must never replace the current view.</p></div>";
#[derive(Default)]
struct Ledger {
    scope: Option<Scope>,
    visit: u64,
    start: Option<Instant>,
    events: Vec<Value>,
    scheduling: Vec<Value>,
    pairs: Vec<Value>,
    requests: Vec<Value>,
    held: Option<(usize, tokio::sync::oneshot::Sender<()>)>,
    quiet: Option<Value>,
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
fn ledger() -> &'static Mutex<Ledger> {
    LEDGER.get_or_init(Default::default)
}
fn refused() -> Error {
    Error::Api("auto_refresh_fixture_unavailable".into())
}
pub(in crate::canvas) fn requested() -> bool {
    body::requested()
        && !body::failure()
        && matches!(
            std::env::var("SUPER_PLATINUM_CANVAS_AUTO_REFRESH_CHECK").as_deref(),
            Ok("success" | "failure")
        )
}
pub(in crate::canvas) fn failure() -> bool {
    requested()
        && std::env::var("SUPER_PLATINUM_CANVAS_AUTO_REFRESH_CHECK").as_deref() == Ok("failure")
}
fn current(s: &ShellState, l: &Ledger) -> bool {
    requested()
        && body::enabled(s, FILE)
        && l.visit == s.shortcut_generation
        && l.scope.as_ref().is_some_and(|scope| scope.current(s))
}
pub(in crate::canvas) fn enabled(s: &ShellState) -> bool {
    requested() && current(s, &ledger().lock().unwrap())
}
fn millis(l: &Ledger) -> u128 {
    l.start.map(|t| t.elapsed().as_millis()).unwrap_or(0)
}
pub(in crate::canvas) fn arm(s: &ShellState) -> Result<(), String> {
    let view = checklist::presentation(s).ok_or("Auto refresh view missing")?;
    if !requested()
        || !body::enabled(s, FILE)
        || view.read != 1
        || view.stale
        || view.loading
        || view.snapshot.is_none()
    {
        return Err("Auto refresh original loaded body unavailable".into());
    }
    let scope = Scope::capture(s).ok_or("Auto refresh owner missing")?;
    let mut l = ledger().lock().unwrap();
    if l.scope.is_some() {
        return Err("Auto refresh already armed".into());
    }
    l.scope = Some(scope);
    l.visit = s.shortcut_generation;
    l.start = Some(Instant::now());
    Ok(())
}
/// Passive timestamps called only at the actual production effect boundaries.
pub(in crate::canvas) fn scheduled(s: &ShellState, visit: u64) {
    if !requested() {
        return;
    }
    let mut l = ledger().lock().unwrap();
    if !current(s, &l) || visit != l.visit {
        return;
    }
    let ms = millis(&l);
    l.scheduling
        .push(json!({"kind":"scheduled","visit":visit,"millis":ms}));
}
pub(in crate::canvas) fn claimed(s: &ShellState, visit: u64, admitted: bool) {
    if !requested() {
        return;
    }
    let mut l = ledger().lock().unwrap();
    if !current(s, &l) || visit != l.visit {
        return;
    }
    let ms = millis(&l);
    let read = checklist::presentation(s).map(|v| v.read);
    l.scheduling
        .push(json!({"kind":"claimed","visit":visit,"millis":ms,"admitted":admitted,"read":read}));
}
/// Exact protected synthetic post-authentication ingress; not a realtime transport test.
pub(in crate::canvas) fn burst(s: &mut ShellState, count: usize) -> Result<(), String> {
    if count != 10 || !enabled(s) {
        return Err("Auto refresh event owner unavailable".into());
    }
    for _ in 0..count {
        let raw = json!({"type":"file_change","file_id":FILE});
        let frame = super_platinum_core::slack::realtime::parse_event(&raw.to_string())
            .ok_or("Invalid original file event")?;
        if !matches!(&frame, super_platinum_core::slack::events::RtEvent::Unknown(r) if r.kind=="file_change" && r.rest.get("file_id").and_then(Value::as_str)==Some(FILE))
        {
            return Err("Unexpected file event".into());
        }
        let before = checklist::presentation(s)
            .ok_or("Missing original view")?
            .read;
        if !checklist::invalidate(s, "T1", FILE) {
            return Err("Original event refused".into());
        }
        let mut l = ledger().lock().unwrap();
        let ms = millis(&l);
        let serial = l.events.len() + 1;
        let visit = l.visit;
        l.events.push(json!({"serial":serial,"team":"T1","frame":raw,"visit":visit,"readBefore":before,"millis":ms,"accepted":true,"ingress":"protected-post-authentication"}));
    }
    Ok(())
}
pub(in crate::canvas) fn release(s: &ShellState, expected: usize) -> Result<(), String> {
    let mut l = ledger().lock().unwrap();
    if !current(s, &l)
        || l.held
            .as_ref()
            .is_none_or(|(serial, _)| *serial != expected)
    {
        return Err("Original held pair unavailable".into());
    }
    let (_, sender) = l.held.take().unwrap();
    let ms = millis(&l);
    l.pairs[expected - 1]["releasedMillis"] = json!(ms);
    sender
        .send(())
        .map_err(|_| "Original pair receiver retired".into())
}
pub(in crate::canvas) fn quiet(s: &ShellState, before: Value, elapsed: u128) -> Result<(), String> {
    let after = snapshot(s);
    if elapsed < 750
        || before["pairs"] != after["pairs"]
        || before["requests"] != after["requests"]
        || before["scheduling"] != after["scheduling"]
    {
        return Err("Error latch retried without user action".into());
    }
    ledger().lock().unwrap().quiet =
        Some(json!({"elapsedMillis":elapsed,"before":before,"after":after}));
    Ok(())
}
pub(in crate::canvas) fn snapshot(s: &ShellState) -> Value {
    let l = ledger().lock().unwrap();
    json!({"case":if failure(){"failure"}else{"success"},"armed":l.scope.is_some(),"current":current(s,&l),
    "visit":l.visit,"events":l.events,"scheduling":l.scheduling,"pairs":l.pairs,"requests":l.requests,
    "held":l.held.as_ref().map(|(serial,_)|*serial),"quiet":l.quiet,"networkCalls":0,"httpCapability":false})
}
pub(in crate::canvas) async fn read_pair(
    state: Signal<ShellState>,
    read: &checklist::Read,
) -> Result<(CanvasCollabSnapshot, (File, CanvasDocument)), Error> {
    let (init, bytes) = body::responses();
    let (serial, failed, old, receiver, file_value) = {
        let s = state.peek();
        if !enabled(&s) || !read.current(&s) {
            return Err(refused());
        }
        let mut l = ledger().lock().unwrap();
        let serial = l.pairs.len() + 1;
        if serial > if failure() { 3 } else { 2 } || l.held.is_some() {
            return Err(refused());
        }
        let old = serial == 1;
        let failed = failure() && serial == 2;
        let mut file = body::file();
        file["title"] = json!(if old { OLD_TITLE } else { TITLE });
        file["name"] = file["title"].clone();
        let owner = body::owner();
        let client = &s.core.client;
        let mut rows = vec![
            body::record(
                api::canvas_info(client, &owner, FILE),
                json!({"ok":true,"file":file}),
            )?,
            body::record(api::canvas_controller_init(client, &owner), init.clone())?,
            body::record(
                api::canvas_collab_load_request(client, &owner, "returned-a-thread")?,
                json!({"base64":STANDARD.encode(&bytes)}),
            )?,
            body::record(
                api::canvas_info(client, &owner, FILE),
                json!({"ok":true,"file":file}),
            )?,
            json!({"target":FILE,"method":"GET","path":file["url_private"],"args":{},"response":if failed{json!({"error":"fixture_auto_refresh_failed"})}else{json!({"html":if old{OLD_HTML}else{HTML}})}}),
        ];
        for r in &mut rows {
            r["serial"] = json!(l.requests.len() + 1);
            r["pair"] = json!(serial);
            l.requests.push(r.clone());
        }
        let ms = millis(&l);
        let v = checklist::presentation(&s).ok_or_else(refused)?;
        l.pairs.push(json!({"serial":serial,"read":v.read,"visit":v.visit,"startedMillis":ms,"status":"pending","readCurrentAtStart":true,"releasedMillis":null}));
        let receiver = if serial <= 2 {
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held = Some((serial, tx));
            Some(rx)
        } else {
            None
        };
        (serial, failed, old, receiver, file)
    };
    if let Some(rx) = receiver {
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| refused())?
            .map_err(|_| refused())?;
    }
    let file: File = serde_json::from_value(file_value).map_err(|_| refused())?;
    let canonical = CanvasCollabSnapshot::from_responses(
        file.clone(),
        CanvasControllerInit::parse(init, &body::owner())?,
        LoadDataResponse::decode(&bytes).map_err(|_| refused())?,
    )?;
    let document = tokio::task::spawn_blocking(move || {
        CanvasDocument::parse(if old { OLD_HTML } else { HTML })
    })
    .await
    .map_err(|_| refused())?
    .map_err(|_| refused())?;
    let s = state.peek();
    let owns = enabled(&s) && read.owns_view(&s);
    let fresh = read.current(&s);
    let mut l = ledger().lock().unwrap();
    let ms = millis(&l);
    l.pairs[serial - 1]["finishedMillis"] = json!(ms);
    l.pairs[serial - 1]["ownerCurrentAtFinish"] = json!(owns);
    l.pairs[serial - 1]["readCurrentAtFinish"] = json!(fresh);
    l.pairs[serial - 1]["status"] = json!(if !owns {
        "retired"
    } else if failed {
        "failed"
    } else if !fresh {
        "stale-success"
    } else {
        "ready"
    });
    if !owns {
        return Err(refused());
    }
    if failed {
        return Err(Error::Api("fixture_auto_refresh_failed".into()));
    }
    // A successful stale response must reach the unchanged loader's original-read guard.
    // Returning a fabricated failure here would incorrectly latch automatic recovery off.
    Ok((canonical, (file, document)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_refresh_variants_decode_without_edit_authority() {
        let (init, bytes) = body::responses();
        let mut value = body::file();
        value["title"] = json!(TITLE);
        let file: File = serde_json::from_value(value).unwrap();
        let snapshot = CanvasCollabSnapshot::from_responses(
            file.clone(),
            CanvasControllerInit::parse(init, &body::owner()).unwrap(),
            LoadDataResponse::decode(&bytes).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.file_id(), FILE);
        assert!(snapshot.sections.is_empty());
        assert_eq!(file.extra["editable"], false);
        for html in [HTML, OLD_HTML] {
            assert!(CanvasDocument::parse(html).is_ok());
        }
        assert!(OLD_HTML.contains(OLD_BODY));
        assert!(!HTML.contains(OLD_BODY));
    }
}
