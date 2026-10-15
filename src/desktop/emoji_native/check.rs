use super::*;
use crate::state::{ChannelOpen, ShellState};
use dioxus::prelude::*;
use std::{path::PathBuf, time::Duration};
fn ensure(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}
pub fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !enabled() {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(150), run(state, &desktop)).await;
        let receipt = match result {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Emoji native check timed out"}),
        };
        eprintln!("emoji-native-check: {receipt}");
    });
}
async fn dom(op: &str, mut input: Value) -> Result<Value, String> {
    input["op"] = json!(op);
    let code = format!(
        "{};try{{dioxus.send(await emojiNative({input}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",
        include_str!("dom.js")
    );
    let mut eval = document::eval(&code);
    let result = tokio::time::timeout(Duration::from_secs(4), eval.recv::<Value>()).await;
    let _ = eval.send(());
    match result {
        Ok(Ok(v)) if v["ok"] == true => Ok(v),
        Ok(Ok(v)) => Err(format!("Emoji native DOM: {v}")),
        _ => Err("Emoji native DOM timeout".into()),
    }
}
async fn wait_dom(op: &str, input: Value) -> Result<Value, String> {
    let mut last = String::new();
    for _ in 0..150 {
        match dom(op, input.clone()).await {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(last)
}
async fn ready() -> Result<(PathBuf, String, String), String> {
    let nonce = std::env::var("SUPER_PLATINUM_EMOJI_NATIVE_NONCE").map_err(|_| "Missing nonce")?;
    ensure(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid nonce",
    )?;
    let dir = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_EMOJI_NATIVE_DIR").ok_or("Missing private directory")?,
    );
    ensure(
        dir.is_absolute()
            && dir.file_name().and_then(|s| s.to_str()) == Some(&format!("emoji-native-{nonce}")),
        "Invalid private directory",
    )?;
    safe_file(&dir, true)?;
    let alias = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    let expected = json!({"version":1,"nonce":nonce,"pid":std::process::id(),"fixture":alias});
    let path = dir.join("ready.json");
    ensure(!path.exists(), "Readiness must be new")?;
    eprintln!("emoji-native-waiting: {expected}");
    for _ in 0..300 {
        let p = path.clone();
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Value>, String> {
            if !p.exists() {
                return Ok(None);
            }
            safe_file(&p, false)?;
            let bytes = std::fs::read(p).map_err(|_| "Readiness read failed")?;
            ensure(bytes.len() <= 1024, "Oversized readiness")?;
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "Malformed readiness".into())
        })
        .await
        .map_err(|_| "Readiness task failed")??;
        if let Some(found) = found {
            ensure(found == expected, "Readiness owner mismatch")?;
            return Ok((dir, nonce, alias));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err("Readiness timed out".into())
}
fn safe_file(path: &std::path::Path, dir: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Missing owned artifact")?;
    ensure(
        !meta.file_type().is_symlink() && if dir { meta.is_dir() } else { meta.is_file() },
        "Invalid owned artifact",
    )?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure(
            meta.file_attributes() & 0x400 == 0,
            "Reparse artifact refused",
        )?;
    }
    Ok(())
}
fn owner(s: &ShellState) -> Value {
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"team":s.core.active_team,
        "channel":s.core.active_channel,"view":s.draft_writes.view_id,"channelGeneration":s.channel_generation,
        "scope":crate::view::composer::field_scope(s,crate::state::ComposerTarget::Channel),
        "mount":crate::view::composer::field_mount_token(s,crate::state::ComposerTarget::Channel),
        "catalog":crate::emoji_catalog::current(s).map(|c|c.token.clone()),
        "preferences":s.core.workspaces["T1"].emoji_preferences})
}
fn messages(s: &ShellState) -> Value {
    json!({"channels":s.core.workspaces["T1"].messages.iter().map(|(id,b)|(id,json!({"messages":b.messages,"pending":b.pending}))).collect::<std::collections::BTreeMap<_,_>>(),
        "threadCount":s.core.threads.len(),"pendingDeliveryCount":s.pending_delivery.len()})
}
async fn read() -> Result<Value, String> {
    Ok(wait_dom("read", json!({})).await?["native"].clone())
}
async fn settle(state: Signal<ShellState>, text: &str) -> Result<Value, String> {
    for _ in 0..150 {
        if let Ok(v) = read().await {
            if v["text"] == text && state.peek().core.composer.text == text {
                return Ok(v);
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Actual native input and model did not settle".into())
}
async fn seed(
    state: Signal<ShellState>,
    text: &str,
    start: usize,
    end: usize,
    direction: &str,
) -> Result<Value, String> {
    dom(
        "seed",
        json!({"text":text,"start":start,"end":end,"direction":direction}),
    )
    .await?;
    settle(state, text).await
}
async fn catalog(state: Signal<ShellState>) -> Result<(), String> {
    crate::emoji_preferences::ensure(state);
    for _ in 0..150 {
        if crate::emoji_catalog::ensure(state).is_some() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Real catalog worker did not publish".into())
}
async fn reached() -> Result<(), String> {
    for _ in 0..150 {
        if PROBE.get().unwrap().lock().unwrap().reached {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Actual admission did not reach hold".into())
}
async fn ack(kind: &str, count: usize) -> Result<(), String> {
    for _ in 0..150 {
        if events().iter().filter(|e| e["kind"] == kind).count() == count {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Actual ACK count did not reach {kind}/{count}"))
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    dir: &std::path::Path,
    name: &str,
    choice: &str,
    captures: &mut serde_json::Map<String, Value>,
    geometry: &mut Vec<Value>,
) -> Result<(), String> {
    let surface = if matches!(name, "completion-offered" | "custom-offered") {
        "completion"
    } else if matches!(name, "picker-selected" | "picker-unready" | "picker-ready") {
        "picker"
    } else {
        "composer"
    };
    let mut value=wait_dom("measure",json!({"name":choice,"surface":surface,"ready":if surface=="picker"{Some(name!="picker-unready")}else{None}})).await?;
    value["stage"] = json!(name);
    value["owner"] = owner(&state.peek());
    value["model"] = json!(state.peek().core.composer);
    geometry.push(value);
    let path = dir.join(format!("{name}.png"));
    eprintln!(
        "emoji-native-capture: {}",
        json!({"stage":name,"edge":"before"})
    );
    let (actual, _, _) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    captures.insert(name.into(), json!(actual.to_string_lossy()));
    eprintln!(
        "emoji-native-capture: {}",
        json!({"stage":name,"edge":"after"})
    );
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (dir, nonce, alias) = ready().await?;
    ensure(
        state.peek().core.session.is_none() && state.peek().core.transport.is_none(),
        "Connected fixture refused",
    )?;
    let initial_messages = messages(&state.peek());
    let mut captures = serde_json::Map::new();
    let mut geometry = vec![];
    let mut actions = vec![];
    catalog(state).await?;
    settle(state, BASE).await?;
    capture(
        state,
        desktop,
        &dir,
        "initial",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    for (name, offered, done) in [
        ("rocket", "completion-offered", "completion-standard"),
        ("party_fixture", "custom-offered", "completion-custom"),
    ] {
        stage(done);
        let text = format!("\u{1f98a} :{name}");
        let n = text.encode_utf16().count();
        seed(state, &text, n, n, "none").await?;
        capture(
            state,
            desktop,
            &dir,
            offered,
            name,
            &mut captures,
            &mut geometry,
        )
        .await?;
        arm("completion-choose");
        let choice = wait_dom("choose", json!({"name":name})).await?;
        reached().await?;
        let before = read().await?;
        release();
        let expected = if name == "rocket" {
            "\u{1f98a} \u{1f680} ".to_owned()
        } else {
            "\u{1f98a} :party_fixture: ".to_owned()
        };
        let after = settle(state, &expected).await?;
        ack("completion-ack", if name == "rocket" { 1 } else { 2 }).await?;
        actions.push(json!({"kind":done,"choice":choice,"before":before,"after":after}));
        capture(
            state,
            desktop,
            &dir,
            done,
            name,
            &mut captures,
            &mut geometry,
        )
        .await?;
    }
    stage("picker-cancelled");
    seed(state, BASE, 3, 10, "backward").await?;
    let before = read().await?;
    let open = dom("open", json!({})).await?;
    wait_dom("search", json!({"name":"rocket"})).await?;
    capture(
        state,
        desktop,
        &dir,
        "picker-selected",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    let escape = dom("escape", json!({})).await?;
    for _ in 0..30 {
        if dom("read", json!({})).await?["pickerPresent"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let after = read().await?;
    ensure(
        before["text"] == after["text"]
            && before["start"] == after["start"]
            && before["end"] == after["end"]
            && before["direction"] == after["direction"],
        "Cancel changed native selection",
    )?;
    actions.push(json!({"kind":"picker-cancelled","open":open,"escape":escape,"before":before,"after":after}));
    capture(
        state,
        desktop,
        &dir,
        "picker-cancelled",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    // Existing fixture ensure supplies authoritative empty preferences only when released.
    stage("picker-inserted");
    PROBE.get().unwrap().lock().unwrap().preferences = true;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .emoji_preferences = Default::default();
    let open = dom("open", json!({})).await?;
    wait_dom("search", json!({"name":"rocket"})).await?;
    capture(
        state,
        desktop,
        &dir,
        "picker-unready",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    let blocked = wait_dom("choose", json!({"name":"rocket"})).await?;
    ensure(
        events().iter().all(|e| e["kind"] != "picker-choose"),
        "Unready choice consumed dialog",
    )?;
    PROBE.get().unwrap().lock().unwrap().preferences = false;
    crate::emoji_preferences::ensure(state);
    capture(
        state,
        desktop,
        &dir,
        "picker-ready",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    arm("picker-choose");
    let choice = dom("choose", json!({"name":"rocket"})).await?;
    reached().await?;
    release();
    let after = settle(state, "\u{1f98a} \u{1f680} end").await?;
    ack("picker-ack", 1).await?;
    actions.push(json!({"kind":"picker-inserted","open":open,"blocked":blocked,"choice":choice,"before":before,"after":after}));
    capture(
        state,
        desktop,
        &dir,
        "picker-inserted",
        "rocket",
        &mut captures,
        &mut geometry,
    )
    .await?;
    for guard in [
        "newer",
        "catalog",
        "account",
        "mount",
        "navigation",
        "composition",
    ] {
        let name = format!("guard-{guard}");
        stage(&name);
        catalog(state).await?;
        // Escape deliberately suppresses the unchanged query. Begin each guard
        // with a real intervening input so identical guard text is a new query.
        seed(state, "\u{1f98a} ", 3, 3, "forward").await?;
        seed(state, "\u{1f98a} :rocket", 10, 10, "none").await?;
        arm("completion-choose");
        let choice = wait_dom("choose", json!({"name":"rocket"})).await?;
        reached().await?;
        let before = read().await?;
        let owner_before = owner(&state.peek());
        let mut composition = Value::Null;
        match guard {
            "newer" => {
                seed(
                    state,
                    NEWER,
                    NEWER.encode_utf16().count(),
                    NEWER.encode_utf16().count(),
                    "forward",
                )
                .await?;
            }
            "catalog" => {
                state
                    .write()
                    .core
                    .workspaces
                    .get_mut("T1")
                    .unwrap()
                    .custom_emoji_revision += 1;
                catalog(state).await?;
            }
            "account" => {
                state.write().core.account_epoch += 1;
                catalog(state).await?;
            }
            "mount" => {
                state.write().draft_writes.view_id += 1;
                catalog(state).await?;
            }
            "navigation" => {
                {
                    let mut s = state.write();
                    let i = s.channels.iter().position(|c| c.id == "C1").unwrap();
                    s.select_channel(i, ChannelOpen::Global);
                }
                read().await?;
                {
                    let mut s = state.write();
                    let i = s.channels.iter().position(|c| c.id == "C2").unwrap();
                    s.select_channel(i, ChannelOpen::Global);
                }
                catalog(state).await?;
            }
            "composition" => {
                composition = dom("compose", json!({})).await?;
            }
            _ => unreachable!(),
        }
        release();
        if matches!(guard, "newer" | "catalog" | "composition") {
            let ticket = events()
                .into_iter()
                .find(|e| e["stage"] == name && e["kind"] == "completion-choose")
                .ok_or("Missing actual chosen ticket")?["value"]["ticket"]
                .clone();
            wait_dom("controller", json!({"ticket":ticket})).await?;
        } else {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let expected = if guard == "newer" {
            NEWER
        } else {
            "\u{1f98a} :rocket"
        };
        let after = settle(state, expected).await?;
        ensure(
            events()
                .iter()
                .filter(|e| e["kind"] == "completion-ack")
                .count()
                == 2,
            "Rejected choice acknowledged insertion",
        )?;
        actions.push(json!({"kind":name,"choice":choice,"before":before,"after":after,"ownerBefore":owner_before,"ownerAfter":owner(&state.peek()),"composition":composition}));
        if guard == "composition" {
            dom("end-compose", json!({})).await?;
        }
        dom("escape", json!({})).await?;
        capture(
            state,
            desktop,
            &dir,
            &name,
            "rocket",
            &mut captures,
            &mut geometry,
        )
        .await?;
    }
    ensure(
        messages(&state.peek()) == initial_messages,
        "Emoji insertion changed chat messages",
    )?;
    let controller_events = dom("controller", json!({})).await?["events"].clone();
    Ok(
        json!({"ok":true,"fixture":alias,"nonce":nonce,"pid":std::process::id(),"noSession":state.peek().core.session.is_none(),"noTransport":state.peek().core.transport.is_none(),"noCache":state.peek().core.cache.is_none(),
        "preparedRequests":0,"attempts":[],"syntheticInput":true,"syntheticPreferences":true,"preferenceWriterCovered":false,
        "captures":captures,"geometry":geometry,"viewport":geometry[0]["viewport"],"actions":actions,"events":events(),"controllerEvents":controller_events,"messagesInitial":initial_messages,"messagesFinal":messages(&state.peek()),
        "limitations":["Synthetic DOM input/clicks/composition; no trusted OS input or IME claim.","Native insertion ACK and real admission only; preference HTTP/writer/throttle not exercised.","No Session or Transport; zero prepared preference requests. No Slack writes.","Channel first cohort; thread insertion, undo/redo, multi-tone, and rich editor parity remain separate gates."]}),
    )
}
