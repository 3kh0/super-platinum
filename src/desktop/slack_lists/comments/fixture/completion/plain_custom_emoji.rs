//! Closed raw emoji.list responses; production owns parsing and Save authority.
use super::*;
use crate::directory_member_fetches::Context;
use std::sync::OnceLock;
use super_platinum_core::slack::api::emoji_catalog::Read;

pub(crate) const TEXT: &str = "Custom :party_parrot: Alias :parrot_alias:";
pub(crate) const RAW: &str = include_str!("plain_custom_emoji/catalog.json");
pub(crate) const FUTURE: &str = include_str!("plain_custom_emoji/future.json");
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && super::super::compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_CUSTOM_EMOJI_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain-custom" | "own-edit-plain-custom-retired")
        )
}
pub(crate) fn retired() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-custom-retired")
}
fn admitted() -> bool {
    named() || super::super::plain::emoji()
}
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<Result<Value, Error>>)>,
    contexts: Vec<Arc<Context>>,
    finished: usize,
    cancelled: usize,
}
fn ledger() -> &'static Mutex<Ledger> {
    static VALUE: OnceLock<Mutex<Ledger>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
struct Flight {
    serial: usize,
    returned: bool,
}
impl Drop for Flight {
    fn drop(&mut self) {
        let mut l = ledger().lock().unwrap();
        l.finished += 1;
        if !self.returned {
            l.cancelled += 1;
        }
        l.requests[self.serial - 1]["settled"] = json!(true);
        l.requests[self.serial - 1]["cancelled"] = json!(!self.returned);
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Option<Result<Value, Error>> {
    if super::super::plain::emoji_display::named() {
        return Some(super::super::plain::emoji_display::read(state, request).await);
    }
    if !admitted() {
        return None;
    }
    Some(read_owned(state, request).await)
}
async fn read_owned(state: Signal<ShellState>, request: &PreparedRequest) -> Result<Value, Error> {
    let (ctx, wire) = {
        let shell = state.peek();
        let m = handle(&shell)?;
        let ctx = Arc::new(Context::capture(&shell, &m.session.team_id)?);
        let expected = Read::new(&m.session).request(&shell.core.client, &m.session)?;
        if !ctx.current(&shell) || !protocol::same_request(request, &expected) {
            return Err(refused());
        }
        (ctx, super::super::own_edit::wire(request)?)
    };
    let (serial, rx) = {
        let mut l = ledger().lock().unwrap();
        let serial = l.requests.len() + 1;
        // The old standard-only refusal now has an explicit failed catalog read.
        // The new successful case also begins with one genuine Offline response.
        let offline = !named() || (!retired() && serial == 1);
        l.requests.push(json!({"serial":serial,"request":wire,"owner":{"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id},"response":null,"settled":false,"cancelled":false,"contextCurrentAtReturn":null,"release":null}));
        l.contexts.push(ctx.clone());
        if offline {
            l.requests[serial - 1]["response"] = json!({"error":"offline"});
            (serial, None)
        } else {
            let (tx, rx) = oneshot::channel();
            l.held.push((serial, tx));
            (serial, Some(rx))
        }
    };
    let mut flight = Flight {
        serial,
        returned: false,
    };
    let result = if let Some(rx) = rx {
        rx.await
            .map_err(|_| Error::Offline("Closed custom emoji receiver retired".into()))?
    } else {
        Err(Error::Offline(
            "Closed full emoji catalog unavailable".into(),
        ))
    };
    ledger().lock().unwrap().requests[serial - 1]["contextCurrentAtReturn"] =
        json!(ctx.current(&state.peek()));
    flight.returned = true;
    result
}
pub(crate) fn release(serial: usize) -> Result<Value, String> {
    if !named() {
        return Err("Closed custom catalog required".into());
    }
    let raw: Value = serde_json::from_str(RAW).map_err(|e| e.to_string())?;
    let tx = {
        let mut l = ledger().lock().unwrap();
        let i = l
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original custom catalog not held")?;
        l.requests[serial - 1]["response"] = json!({"raw":raw,"bytes":RAW});
        l.held.remove(i).1
    };
    let receiver_retired = tx.is_closed();
    let delivered = tx.send(Ok(raw)).is_ok();
    let proof = json!({"serial":serial,"receiverRetired":receiver_retired,"delivered":delivered});
    ledger().lock().unwrap().requests[serial - 1]["release"] = proof.clone();
    Ok(proof)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let l = ledger().lock().unwrap();
    json!({"requests":l.requests,"pending":l.held.iter().map(|(n,_)|*n).collect::<Vec<_>>(),"started":l.requests.len(),"finished":l.finished,"active":l.requests.len()-l.finished,"cancelled":l.cancelled,"contextsCurrent":l.contexts.iter().map(|c|c.current(shell)).collect::<Vec<_>>(),"networkCalls":0,"httpCapability":false})
}
