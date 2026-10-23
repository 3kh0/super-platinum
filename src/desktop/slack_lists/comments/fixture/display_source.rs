//! Closed supplied-URL appearance; no workspace emoji publication or network.
use super::*;
use crate::media::MediaUri;
use super_platinum_core::MediaAssetKind;
pub(crate) const CASE: &str = "rich-emoji-display-source";
pub(crate) const TS: &str = "1781524801.000001";
pub(crate) const URL_A: &str = "https://fixture.invalid/shared-emoji-a.png";
pub(crate) const URL_B: &str = "https://fixture.invalid/shared-emoji-b.png";
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && compose::check_enabled()
        && std::env::var("SUPER_PLATINUM_LIST_EMOJI_DISPLAY_SOURCE_CHECK").as_deref() == Ok("1")
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok(CASE)
}
pub(crate) fn blocks(replacement: bool) -> Value {
    json!([{"type":"rich_text","block_id":"original-external-emoji","opaque":{"keep":true},"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Shared "},{"type":"emoji","name":"external_original","display_alias":"shared_party","display_url":if replacement{URL_B}else{URL_A},"display_team_id":"TFOREIGN001","opaque":{"keep":"original appearance"}},{"type":"text","text":" Standard "},{"type":"emoji","name":"wave"}]}]}])
}
pub(crate) fn decorate(ts: &str, next: bool, response: &mut Value) {
    if !named() || next || ts != ROOT {
        return;
    }
    let row = &mut response["messages"][1];
    assert_eq!(row["ts"], TS);
    row["text"] = json!("Shared :external_original: Standard :wave:");
    row["blocks"] = blocks(false);
    row["display_fixture_opaque"] = json!({"keep":"received"});
}
pub(crate) fn replacement() -> Value {
    json!({"type":"message","subtype":"message_changed","channel":"CCOMMENT01","message":{"ts":TS,"thread_ts":ROOT,"user":"U1","text":"Shared :external_original: Standard :wave:","blocks":blocks(true)}})
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let w = shell.core.workspaces.get("T1");
    json!({"named":named(),"noSession":shell.core.session.is_none(),"noTransport":shell.core.transport.is_none(),"customEmoji":w.map(|w|&w.custom_emoji),"customRevision":w.map(|w|w.custom_emoji_revision),"mediaEpoch":shell.media_epoch})
}
pub(crate) fn deliver_media(shell: &ShellState, replacement: bool) -> Result<Value, String> {
    if !named() || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Closed original display media required".into());
    }
    let url = if replacement { URL_B } else { URL_A };
    let id = shell
        .media
        .register_image(MediaAssetKind::Emoji, url, "image/png");
    if shell.media.is_ready(&id) {
        return Err("Pending original URL already has bytes".into());
    }
    let old = shell
        .media
        .register_image(MediaAssetKind::Emoji, URL_A, "image/png");
    if replacement && (old == id || !shell.media.is_ready(&old)) {
        return Err("Replacement must retain distinct original ready asset".into());
    }
    let bytes: &[u8] = if replacement {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/png/icon_32.png"
        ))
    } else {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/png/icon_16.png"
        ))
    };
    let uri = id.webview_uri_at(shell.media_epoch);
    shell
        .media
        .insert(id.clone(), "image/png", Arc::<[u8]>::from(bytes));
    Ok(
        json!({"url":url,"id":id.uri(),"uriBefore":uri,"epochBefore":shell.media_epoch,"readyBefore":false,"readyAfter":shell.media.is_ready(&id),"originalId":old.uri(),"originalStillReady":shell.media.is_ready(&old),"replacement":replacement,"bytes":bytes.len(),"mime":"image/png"}),
    )
}
