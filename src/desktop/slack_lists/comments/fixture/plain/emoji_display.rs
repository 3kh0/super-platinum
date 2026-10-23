//! Closed complete emoji display sources; production owns every decode/adoption.
use super::*;
use crate::directory_member_fetches::Context;
use crate::media::MediaUri;
use std::sync::OnceLock;
use super_platinum_core::slack::api::emoji_catalog::Read;
pub(crate) const INITIAL: &str = include_str!("emoji_display/initial.json");
pub(crate) const FUTURE: &str = include_str!("emoji_display/future.json");
pub(crate) const RAW: &str = include_str!("emoji_display/catalog.json");
const URL: &str = "https://fixture.invalid/list-display-parrot.png";
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_PLAIN_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_OWN_EDIT_EMOJI_DISPLAY_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("own-edit-plain-emoji-display" | "own-edit-plain-emoji-display-recovery")
        )
}
pub(crate) fn recovery() -> bool {
    named()
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("own-edit-plain-emoji-display-recovery")
}
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    contexts: Vec<Context>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static VALUE: OnceLock<Mutex<Ledger>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    if !named() {
        return Err(Error::Offline("Closed display catalog required".into()));
    }
    let shell = state.peek();
    let ctx = Context::capture(&shell, "T1")?;
    let expected = Read::new(&ctx.session).request(&ctx.client, &ctx.session)?;
    if !ctx.current(&shell) || !completion::protocol::same_request(request, &expected) {
        return Err(Error::Offline(
            "Original display catalog factory changed".into(),
        ));
    }
    let wire = own_edit::wire(request)?;
    let mut l = ledger().lock().unwrap();
    let serial = l.requests.len() + 1;
    let failed = recovery() && serial == 1;
    let raw: Value =
        serde_json::from_str(RAW).map_err(|_| Error::Api("fixture_display_raw".into()))?;
    l.requests.push(json!({"serial":serial,"request":wire,"owner":{"account":ctx.account,"epoch":ctx.epoch,"team":ctx.session.team_id,"user":ctx.session.user_id},"response":if failed{json!({"error":"offline"})}else{json!({"raw":raw,"bytes":RAW})},"settled":true,"contextCurrentAtReturn":true}));
    l.contexts.push(ctx);
    if failed {
        Err(Error::Offline(
            "Original display catalog unavailable".into(),
        ))
    } else {
        Ok(raw)
    }
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let l = ledger().lock().unwrap();
    json!({"requests":l.requests,"contextsCurrent":l.contexts.iter().map(|c|c.current(shell)).collect::<Vec<_>>(),"networkCalls":0,"httpCapability":false})
}
pub(crate) fn deliver_media(shell: &ShellState) -> Result<Value, String> {
    if !named() {
        return Err("Closed display media required".into());
    }
    let account = shell
        .core
        .active_account
        .as_deref()
        .ok_or("Original account absent")?;
    let slot = format!("list-comment/{account}/T1/parrot_alias");
    let id = shell.media.register_emoji(&slot, URL);
    if shell.media.is_ready(&id) {
        return Err("Original pending media already ready".into());
    }
    let before = id.webview_uri_at(shell.media_epoch);
    let bytes = &include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/png/icon_16.png"
    ))[..];
    shell
        .media
        .insert(id.clone(), "image/png", Arc::<[u8]>::from(bytes));
    Ok(
        json!({"slot":slot,"url":URL,"id":id.uri(),"uriBefore":before,"epochBefore":shell.media_epoch,"readyBefore":false,"readyAfter":shell.media.is_ready(&id),"mime":"image/png","bytes":bytes.len()}),
    )
}
