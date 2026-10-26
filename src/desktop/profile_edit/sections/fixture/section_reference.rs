//! Finite original section endpoint responses through the production reader.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::{PreparedRequest, api::CanvasSectionsRequest};
pub(crate) mod rich;
pub(crate) mod status;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CANVAS_SECTION_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_CANVAS_SECTION_CASE";
pub(crate) const IDS: [&str; 2] = [
    "temp:C:0123456789abcdef01234567",
    "temp:C:89abcdef0123456701234567",
];
pub(crate) const TITLE: &str = "Section canvas";
#[derive(Clone, Copy, Debug)]
pub(crate) enum Reply {
    Present,
    Missing,
    Failure,
    Changed,
}
struct Ledger {
    requests: Vec<Value>,
    completed: usize,
    cancelled: usize,
    held: Option<tokio::sync::oneshot::Sender<Result<Value, Error>>>,
    hold: bool,
    reply: Reply,
}
fn ledger() -> &'static Mutex<Ledger> {
    static STATE: OnceLock<Mutex<Ledger>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(Ledger {
            requests: vec![],
            completed: 0,
            cancelled: 0,
            held: None,
            hold: true,
            reply: Reply::Present,
        })
    })
}
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && [FLAG, mentions::FLAG, files::FLAG, files::BLOCKS_FLAG]
            .iter()
            .all(|f| std::env::var(f).as_deref() == Ok("1"))
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() != Ok("list")
        && matches!(
            case().as_str(),
            "success" | "missing" | "failure" | "cancel" | "retirement" | "status"
        )
        && [
            deletion_event::FLAG,
            deleted_reference::FLAG,
            unavailable_reference::FLAG,
            group_title::FLAG,
            missing_group::FLAG,
            channel_title::FLAG,
            missing_channel::FLAG,
            user_title::FLAG,
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            completion::FLAG,
            group_recovery::FLAG,
        ]
        .iter()
        .all(|f| std::env::var(f).as_deref() != Ok("1"))
}
pub(crate) fn configure(reply: Reply, hold: bool) {
    let mut l = ledger().lock().unwrap();
    l.reply = reply;
    l.hold = hold;
}
pub(crate) fn file_response() -> Value {
    if case() == "status" {
        return json!({"ok":false,"error":"template_not_visible","opaque":{"sectionStatus":true}});
    }
    json!({"ok":true,"file":{"id":files::ID,"mode":"quip","filetype":"quip","title":TITLE,"updated":101,"opaque":{"retained":true}}})
}
fn response(reply: Reply) -> Result<Value, Error> {
    if matches!(reply, Reply::Failure) {
        return Err(Error::Offline("closed section fixture".into()));
    }
    if matches!(reply, Reply::Missing) {
        return Ok(json!({"ok":true,"sections":[{"id":IDS[0],"rich_text_blocks":[]}]}));
    }
    let rows=IDS.iter().enumerate().map(|(i,id)| {
        let text=if matches!(reply,Reply::Changed) { format!("Changed section {}\nIgnored second line",i+1) }
            else { format!("{}{} tail\nIgnored second line","a".repeat(24),if i==0{"\u{1f600}"}else{"\u{1f980}"}) };
        if matches!(reply, Reply::Changed) && case()=="success" { return json!({"id":id,"rich_text_blocks":[rich::mixed()]}); }
        json!({"id":id,"rich_text_blocks":[{"type":"rich_text","opaque":{"keep":i},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":{"bold":true}}]},{"type":"rich_text_section","elements":[{"type":"text","text":"Ignored top level"}]}]}]})
    }).collect::<Vec<_>>();
    Ok(json!({"ok":true,"sections":rows}))
}
pub(crate) fn expected(which: Reply) -> Vec<Vec<u16>> {
    IDS.iter()
        .enumerate()
        .map(|(i, _)| {
            let mut units = format!("{TITLE}: ").encode_utf16().collect::<Vec<_>>();
            match which {
                Reply::Present => {
                    units.extend("a".repeat(24).encode_utf16());
                    units.push(if i == 0 { 0xd83d } else { 0xd83e });
                    units.push(0x2026);
                }
                Reply::Changed if case() == "success" => {
                    let mut preview = "@U #c @g :wave: Section canvas"
                        .encode_utf16()
                        .collect::<Vec<_>>();
                    preview.truncate(25);
                    preview.push(0x2026);
                    units.extend(preview);
                }
                Reply::Changed => units.extend(format!("Changed section {}", i + 1).encode_utf16()),
                _ => units.push(0x2026),
            }
            units
        })
        .collect()
}
pub(crate) fn blocks(text: &str) -> Vec<Value> {
    let token = |i: usize, style: Value| json!({"type":"canvas","file_id":files::ID,"section_id":IDS[i],"text":files::URL,"url":files::URL,"fixture_occurrence":i+1,"style":style,"opaque":{"keep":[i+1,"original"]}});
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text},token(0,json!({"bold":true})),token(1,json!({"italic":true}))]}]}),
    ]
}
pub(crate) fn status() -> Value {
    let l = ledger().lock().unwrap();
    json!({"requests":l.requests,"completed":l.completed,"cancelled":l.cancelled,"held":l.held.is_some(),"active":l.requests.len()-l.completed,"networkCalls":0})
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or_default();
    json!({"case":case(),"files":files::status(),"sections":status(),"sectionSource":super::super::rich::files::sections::fixture_status(shell),"richPreview":rich::snapshot(shell),"account":shell.core.active_account,"epoch":shell.core.account_epoch,"socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),"networkCalls":0})
}
pub(crate) fn release() -> Value {
    let (tx, reply) = {
        let mut l = ledger().lock().unwrap();
        (l.held.take(), l.reply)
    };
    let cancelled = tx.as_ref().is_some_and(|tx| tx.is_closed());
    let result = response(reply);
    let raw = match &result {
        Ok(v) => json!({"raw":v}),
        Err(_) => json!({"error":"offline"}),
    };
    if let Some(row) = ledger().lock().unwrap().requests.last_mut() {
        row["releasedResponse"] = raw;
    }
    let delivered = tx.is_some_and(|tx| tx.send(result).is_ok());
    json!({"delivered":delivered,"cancelledBeforeReply":cancelled})
}
struct Completion {
    serial: usize,
    done: bool,
}
impl Drop for Completion {
    fn drop(&mut self) {
        let mut l = ledger().lock().unwrap();
        l.completed += 1;
        if !self.done {
            l.cancelled += 1;
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    prepared: &PreparedRequest,
    request: &CanvasSectionsRequest,
) -> Result<Value, Error> {
    if !named() || !files::enabled(&state.peek()) {
        return Err(Error::Api("closed_section_fixture".into()));
    }
    let owner = crate::directory_member_fetches::Context::capture(
        &state.peek(),
        state
            .peek()
            .core
            .active_team
            .as_deref()
            .ok_or_else(|| Error::Api("closed_section_fixture".into()))?,
    )?;
    let expected = request.prepare(&owner.client, &owner.session);
    if !files::validate(prepared, &expected)
        || request.file_id() != files::ID
        || request
            .section_ids()
            .iter()
            .any(|id| !IDS.contains(&id.as_str()))
    {
        return Err(Error::Api("closed_section_request".into()));
    }
    let (serial, rx, reply) = {
        let mut l = ledger().lock().unwrap();
        if l.held.is_some() {
            return Err(Error::Api("closed_section_busy".into()));
        }
        let serial = l.requests.len() + 1;
        l.requests.push(json!({"serial":serial,"method":"quip.getCanvasBulkSectionIdsData","args":{"file_id":request.file_id(),"section_ids":request.section_ids().join(",")},"socketGeneration":owner.realtime_generation()}));
        let rx = if l.hold {
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (serial, rx, l.reply)
    };
    let mut guard = Completion {
        serial,
        done: false,
    };
    let result = if let Some(rx) = rx {
        rx.await
            .map_err(|_| Error::Api("closed_section_cancelled".into()))?
    } else {
        response(reply)
    };
    ledger().lock().unwrap().requests[guard.serial - 1]["response"] = match &result {
        Ok(v) => json!({"raw":v}),
        Err(_) => json!({"error":"offline"}),
    };
    guard.done = true;
    result
}
