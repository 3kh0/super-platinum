use super::*;
use dioxus::prelude::*;
use std::{path::PathBuf, sync::Arc, time::Duration};
#[path = "ready.rs"]
mod ready;
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
pub fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(150), run(state, &desktop)).await;
        let failed_js = if matches!(&result, Ok(Ok(_))) {
            None
        } else {
            tokio::time::timeout(Duration::from_secs(2), call("evidence", ""))
                .await
                .ok()
                .and_then(Result::ok)
        };
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"error":error,"rust":evidence(),"js":failed_js}),
            Err(_) => {
                json!({"ok":false,"error":"Native cold deadline","rust":evidence(),"js":failed_js})
            }
        };
        eprintln!("emoji-lifecycle-check: {receipt}");
    });
}
async fn eval(code: &str) -> Result<Value, String> {
    let mut bridge = document::eval(code);
    let value = tokio::time::timeout(Duration::from_secs(20), bridge.recv::<Value>())
        .await
        .map_err(|_| "DOM timeout")?
        .map_err(|_| "DOM evaluator ended")?;
    let _ = bridge.send(());
    if value["ok"] != true {
        record("dom-failure", value.clone());
    }
    ensure(
        value["ok"] == true,
        "DOM check failed; retained structured ledger",
    )?;
    Ok(value)
}
async fn call(op: &str, path: &str) -> Result<Value, String> {
    eval(&format!("try{{dioxus.send(await emojiLifecycle({},{},dioxus));}}catch(error){{dioxus.send({{ok:false,error:String(error),evidence:window.__spEmojiLifecycle?.evidence()}});}}await dioxus.recv();",json!(op),json!(path))).await
}
fn owner(shell: &ShellState) -> Value {
    json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"team":shell.core.active_team,
        "channel":shell.core.active_channel,"channelGeneration":shell.channel_generation,
        "scope":crate::view::composer::field_scope(shell,crate::state::ComposerTarget::Channel),
        "mount":crate::view::composer::field_mount_token(shell,crate::state::ComposerTarget::Channel),
        "preferences":shell.core.workspaces["T1"].emoji_preferences})
}
fn current(state: Signal<ShellState>, expected: &Value) -> bool {
    let shell = state.peek();
    owner(&shell) == *expected
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell.core.cache.is_none()
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    directory: &std::path::Path,
    stage: &str,
    path: &str,
) -> Result<Value, String> {
    let surface = if stage == "initial" {
        "composer"
    } else if path == "colon" {
        "completion"
    } else {
        "picker"
    };
    let code = format!(
        "{};try{{dioxus.send(await emojiNative({}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",
        include_str!("../../emoji_native/dom.js"),
        json!({"op":"measure","name":"rocket","surface":surface})
    );
    let geometry = eval(&code).await?;
    eprintln!(
        "emoji-lifecycle-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let path = directory.join(format!("{stage}.png"));
    let (actual, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    eprintln!(
        "emoji-lifecycle-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(
        json!({"stage":stage,"path":actual.to_string_lossy(),"geometry":geometry,"owner":owner(&state.peek())}),
    )
}
async fn refresh(
    mut state: Signal<ShellState>,
    path: &str,
    expected_owner: &Value,
) -> Result<Value, String> {
    let mut bridge = document::eval(&format!(
        "try{{dioxus.send(await emojiLifecycle('refresh',{},dioxus));}}catch(error){{dioxus.send({{ok:false,error:String(error),evidence:window.__spEmojiLifecycle?.evidence()}});}}await dioxus.recv();",
        json!(path)
    ));
    let armed = tokio::time::timeout(Duration::from_secs(5), bridge.recv::<Value>())
        .await
        .map_err(|_| "Refresh arm timeout")?
        .map_err(|_| "Refresh evaluator ended")?;
    ensure(
        armed == json!({"kind":"refresh-armed"}),
        "Wrong refresh arm",
    )?;
    ensure(current(state, expected_owner), "Refresh owner changed")?;
    // Two bounded canonical edits. No full-map clone or teardown in this input path.
    {
        let mut shell = state.write();
        let workspace = shell.core.workspaces.get_mut("T1").unwrap();
        workspace
            .custom_emoji
            .get_mut(&corpus::name(0))
            .unwrap()
            .value
            .push_str("&native_refresh=1");
        workspace
            .custom_emoji
            .get_mut(&corpus::name(3))
            .unwrap()
            .value = format!("alias:{}", corpus::name(1));
        workspace.custom_emoji_revision += 1;
    }
    record(
        "canonical-refresh",
        json!({"revision":2,"changed":[corpus::name(0),corpus::name(3)]}),
    );
    bridge.send(()).map_err(|_| "Refresh ACK failed")?;
    let value = tokio::time::timeout(Duration::from_secs(20), bridge.recv::<Value>())
        .await
        .map_err(|_| "Refresh result timeout")?
        .map_err(|_| "Refresh evaluator ended")?;
    let _ = bridge.send(());
    if value["ok"] != true {
        record("dom-failure", value.clone());
    }
    ensure(
        value["ok"] == true,
        "Refresh DOM failed; retained structured ledger",
    )?;
    Ok(value)
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (directory, nonce, alias) = ready::ready().await?;
    let path = fixture(&alias).ok_or("Unknown exact cold fixture")?;
    let expected_owner = owner(&state.peek());
    ensure(
        expected_owner["account"] == ACCOUNT
            && expected_owner["team"] == "T1"
            && current(state, &expected_owner),
        "Connected or wrong fixture",
    )?;
    let install = format!(
        "{}\n{}\ntry{{dioxus.send(await emojiLifecycle('install',{},dioxus));}}catch(e){{dioxus.send({{ok:false,error:String(e)}});}}await dioxus.recv();",
        include_str!("probe.js"),
        include_str!("driver.js"),
        json!(path)
    );
    eval(&install).await?;
    let mut mounted = false;
    for _ in 0..100 {
        if call("ready", path).await.is_ok_and(|v| v["ready"] == true) {
            mounted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(mounted, "Cold composer not mounted")?;
    let expected_owner = owner(&state.peek());
    let mut captures = Vec::new();
    captures.push(capture(state, desktop, &directory, "initial", path).await?);
    let cold_state = crate::emoji_catalog::probe_state(&state.peek());
    ensure(
        cold_state["serial"] == 0
            && cold_state["ready"] == 0
            && cold_state["building"] == 0
            && cold_state["attempted"] == 0,
        "Catalog was prewarmed",
    )?;
    ensure(
        evidence()["events"].as_array().is_some_and(Vec::is_empty),
        "Ensure/build happened before cold action",
    )?;
    phase("cold");
    let cold = call("cold", path).await?;
    eprintln!(
        "emoji-lifecycle-progress: {}",
        json!({"phase":"cold","pid":std::process::id(),"nonce":nonce,"fixture":alias,"js":cold,"rust":evidence()})
    );
    ensure(current(state, &expected_owner), "Cold owner changed")?;
    let held = crate::emoji_catalog::current(&state.peek()).ok_or("Cold publication missing")?;
    ensure(held.token == "1/local", "Unexpected first catalog ticket")?;
    let expected_text = if path == "colon" { ":ro" } else { "" };
    ensure(
        state.peek().core.composer.text == expected_text,
        "Cold action changed model unexpectedly",
    )?;
    captures.push(capture(state, desktop, &directory, "cold-ready", path).await?);
    let alias_name = corpus::name(3);
    let old_url = held
        .catalog
        .resolve(&alias_name, None)
        .and_then(|r| r.image_url)
        .ok_or("Old alias did not resolve")?;
    // The fixture's explicit reader now holds the old Loaded through a real revision.
    phase("refresh");
    let refreshed = refresh(state, path, &expected_owner).await?;
    eprintln!(
        "emoji-lifecycle-progress: {}",
        json!({"phase":"refresh","pid":std::process::id(),"nonce":nonce,"fixture":alias,"js":refreshed,"rust":evidence()})
    );
    ensure(current(state, &expected_owner), "Refresh owner changed")?;
    let replacement =
        crate::emoji_catalog::current(&state.peek()).ok_or("Refresh publication missing")?;
    ensure(
        replacement.token == "2/local"
            && !held.current(&state.peek())
            && replacement.current(&state.peek()),
        "Old/current revision guards failed",
    )?;
    let expected_url = state.peek().core.workspaces["T1"].custom_emoji[&corpus::name(1)]
        .value
        .clone();
    ensure(
        replacement
            .catalog
            .resolve(&alias_name, None)
            .and_then(|r| r.image_url)
            == Some(expected_url),
        "Replacement alias did not refresh",
    )?;
    ensure(
        held.catalog
            .resolve(&alias_name, None)
            .and_then(|r| r.image_url)
            == Some(old_url),
        "Old immutable reader changed",
    )?;
    drop(replacement);
    captures.push(capture(state, desktop, &directory, "refresh-ready", path).await?);
    // Wait for actual controller/watch/picker readers to release the old version.
    // Never report last-owner destruction if another Loaded/catalog/picker exists.
    let mut counts = crate::emoji_catalog::probe_counts(&held);
    for _ in 0..200 {
        if counts == json!({"loaded":1,"catalog":1,"picker":1}) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        counts = crate::emoji_catalog::probe_counts(&held);
    }
    record("controlled-reader-counts", counts.clone());
    ensure(
        counts == json!({"loaded":1,"catalog":1,"picker":1}),
        "Actual readers still retain old catalog; no final-drop timing accepted",
    )?;
    let weak = Arc::downgrade(&held);
    phase("retirement");
    let at = begin();
    drop(held);
    finish(
        at,
        "controlled-final-reader-release",
        || json!({"countsBefore":counts,"weakDead":weak.upgrade().is_none(),"includesPicker":true}),
    );
    ensure(
        weak.upgrade().is_none(),
        "Controlled reader was not final owner",
    )?;
    captures.push(capture(state, desktop, &directory, "retired", path).await?);
    let final_js = call("evidence", path).await?;
    ensure(
        current(state, &expected_owner) && state.peek().core.composer.text == expected_text,
        "Final owner/model changed",
    )?;
    let rust = evidence();
    ensure(rust["overflow"] == false, "Rust ledger overflow")?;
    Ok(
        json!({"ok":true,"version":1,"fixture":alias,"nonce":nonce,"pid":std::process::id(),"path":path,"owner":expected_owner,
        "coldState":cold_state,"cold":cold,"refresh":refreshed,"js":final_js,"rust":rust,"captures":captures,
        "noSession":true,"noTransport":true,"noCache":true,"controlledFinalReader":true,"syntheticInput":true}),
    )
}
