use super::{actions::Actions, control, ledger, native};
use crate::state::ShellState;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    version: u64,
    nonce: String,
    pid: u32,
    creation: u64,
    fixture: String,
    process: String,
    mapping: String,
    event: Option<String>,
}
fn ensure(ok: bool, error: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(error.into()) }
}
fn safe_file(path: &Path, directory: bool) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "Missing owned artifact")?;
    ensure(
        !meta.file_type().is_symlink()
            && if directory {
                meta.is_dir()
            } else {
                meta.is_file()
            },
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
async fn handshake() -> Result<(Ready, PathBuf, Option<native::Event>), String> {
    let nonce =
        std::env::var("SUPER_PLATINUM_EMOJI_RETIREMENT_NONCE").map_err(|_| "Missing nonce")?;
    ensure(
        nonce.len() == 32
            && nonce
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "Invalid nonce",
    )?;
    let process = std::env::var("SUPER_PLATINUM_EMOJI_RETIREMENT_PROCESS")
        .map_err(|_| "Missing process role")?;
    let ordinal = ["W1", "B1", "W2", "B2", "W3", "B3"]
        .iter()
        .position(|p| *p == process)
        .ok_or("Unknown process role")? as u64
        + 1;
    let directory = PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_EMOJI_RETIREMENT_DIR")
            .ok_or("Missing private directory")?,
    );
    ensure(
        directory.is_absolute()
            && directory.file_name().and_then(|n| n.to_str())
                == Some(&format!("emoji-retirement-{nonce}")),
        "Invalid private directory",
    )?;
    let inspected = directory.clone();
    tokio::task::spawn_blocking(move || safe_file(&inspected, true))
        .await
        .map_err(|_| "Directory check failed")??;
    let creation = native::creation_filetime()?;
    let expected = json!({"version":2,"nonce":nonce,"pid":std::process::id(),"creation":creation,"fixture":"emoji-retirement-64000","process":process});
    eprintln!("emoji-retirement-waiting: {expected}");
    let ready = tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let file = directory.join("ready.json");
            let value = tokio::task::spawn_blocking(move || -> Result<Option<Ready>, String> {
                if !file.exists() {
                    return Ok(None);
                }
                safe_file(&file, false)?;
                let bytes = std::fs::read(file).map_err(|_| "Readiness read failed")?;
                ensure(bytes.len() <= 4096, "Oversized readiness")?;
                serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|_| "Malformed readiness".into())
            })
            .await
            .map_err(|_| "Readiness worker failed")??;
            if let Some(value) = value {
                return Ok::<_, String>(value);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "Readiness timeout")??;
    ensure(
        ready.version == 2
            && ready.nonce == nonce
            && ready.pid == std::process::id()
            && ready.creation == creation
            && ready.fixture == "emoji-retirement-64000"
            && ready.process == process,
        "Readiness identity mismatch",
    )?;
    ensure(
        ready.event.is_some() == process.starts_with('B'),
        "Wrong event role",
    )?;
    let mapping = native::parse_handle(&ready.mapping)?;
    let event = ready
        .event
        .as_deref()
        .map(native::parse_handle)
        .transpose()?;
    ensure(event != Some(mapping), "Aliased transferred handles")?;
    let nonce_words = [
        u64::from_str_radix(&nonce[..16], 16).unwrap(),
        u64::from_str_radix(&nonce[16..], 16).unwrap(),
    ];
    // Published readiness transferred handle ownership; never duplicate a reader.
    let event = event
        .map(|h| unsafe { native::Event::from_transferred(h) })
        .transpose()?;
    unsafe { native::attach_mapping(mapping, nonce_words, ordinal) }?;
    Ok((ready, directory, event))
}

pub(super) fn start(state: Signal<ShellState>, _desktop: dioxus::desktop::DesktopContext) {
    dioxus::core::spawn_forever(async move {
        let result = tokio::time::timeout(Duration::from_secs(600), Box::pin(run(state))).await;
        if !matches!(result, Ok(Ok(()))) {
            let error = match result {
                Ok(Err(e)) => e,
                _ => "Process protocol timeout".into(),
            };
            ledger::emit(ledger::event::FAILURE, 0, 0, 40);
            let partial = tokio::time::timeout(
                Duration::from_secs(2),
                eval("({ok:true,evidence:window.__spEmojiRetirement?.evidence()})"),
            )
            .await
            .ok()
            .and_then(Result::ok);
            eprintln!(
                "emoji-retirement-terminal: {}",
                json!({"ok":false,"error":error,"partial":partial})
            );
        }
    });
}
async fn eval(expression: &str) -> Result<Value, String> {
    let mut bridge = document::eval(&format!(
        "try{{dioxus.send(await ({expression}));}}catch(e){{dioxus.send({{ok:false,error:String(e)}})}}await dioxus.recv();"
    ));
    let result = tokio::time::timeout(Duration::from_secs(5), bridge.recv::<Value>())
        .await
        .map_err(|_| "Evaluation timeout")?
        .map_err(|_| "Evaluation ended")?;
    let _ = bridge.send(());
    ensure(result["ok"] == true, "Evaluation failed")?;
    Ok(result)
}
async fn run(state: Signal<ShellState>) -> Result<(), String> {
    let (ready, _directory, event) = handshake().await?;
    let schedule: Value =
        serde_json::from_str(include_str!("schedule.json")).map_err(|_| "Bad frozen schedule")?;
    let process = schedule["processes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["process"] == ready.process)
        .ok_or("Missing process schedule")?;
    let operations = process["ordered_actions"]
        .as_array()
        .ok_or("Missing operation sequence")?;
    let installation = format!(
        "(()=>{{{}\n{}\nif(window.__spEmojiRetirement)throw Error('Duplicate retirement probe');window.__spEmojiRetirement=createEmojiRetirementProbe({{document,clock:()=>performance.now()}});return {{ok:true}};}})()",
        include_str!("probe.js"),
        include_str!("driver.js")
    );
    eval(&installation).await?;
    let mut actor = Actions {
        account: 0,
        process: ready.process.clone(),
        next: None,
        held: 0,
        event,
        targets: BTreeMap::new(),
    };
    let mut completed = Vec::<Value>::new();
    let mut starts = BTreeMap::<String, u64>::new();
    let mut index = 0u64;
    for operation in operations {
        if operation["kind"] == "cutoff" {
            let final_id = operation["native_final_action"]
                .as_str()
                .ok_or("Missing final action")?;
            let start = *starts.get(final_id).ok_or("Final action not started")?;
            let deadline = start + 5_000_000_000;
            let remaining = deadline.saturating_sub(ledger::elapsed());
            if remaining > 0 {
                tokio::time::sleep(Duration::from_nanos(remaining)).await;
            }
            let key = final_id.rsplit_once('/').unwrap().0;
            ensure(actor.targets.contains_key(key), "Subcase missing target")?;
            ledger::emit(
                ledger::event::CUTOFF,
                actor.targets[key],
                0,
                ledger::scalar_id(key),
            );
            continue;
        }
        index += 1;
        if operation["kind"] == "shutdown" {
            let target = *actor
                .targets
                .get(&format!("{}/shutdown", ready.process))
                .ok_or("Missing shutdown target")?;
            if ready.process.starts_with('W') {
                let query_id = format!("{}/setup/shutdown/query", ready.process);
                let query = completed
                    .iter()
                    .find(|r| r["id"] == query_id)
                    .ok_or("Missing actual shutdown query")?;
                let mount = ledger::scalar_id(
                    query["accepted"]["owner"]
                        .as_str()
                        .ok_or("Missing shutdown worker owner")?,
                );
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !control::waiting(target, mount) {
                        control::changed().await;
                    }
                })
                .await
                .map_err(|_| "Worker not waiting at close")?;
            }
            ledger::action(index);
            let evidence =
                eval("({ok:true,evidence:window.__spEmojiRetirement.evidence()})").await?;
            eprintln!(
                "emoji-retirement-close-ready: {}",
                json!({"ok":true,"ready":ready,"index":index,"action":operation["id"],"actions":completed,"targets":actor.targets,"js":evidence,"state":super::actions::info(&state.peek())})
            );
            // Ordinary WM_CLOSE belongs to the external runner. No close service,
            // state reset, explicit reader teardown or successful-drain claim.
            return Ok(());
        }
        let mut action = operation.clone();
        action["index"] = index.into();
        let id = action["id"].as_str().unwrap().to_owned();
        let script = format!(
            "try{{dioxus.send(await window.emojiRetirementAction({},dioxus));}}catch(e){{dioxus.send({{ok:false,error:String(e),id:{}}});}}await dioxus.recv();",
            action,
            json!(id)
        );
        let mut bridge = document::eval(&script);
        let note = tokio::time::timeout(Duration::from_secs(5), bridge.recv::<Value>())
            .await
            .map_err(|_| "Missing action start")?
            .map_err(|_| "Action bridge ended")?;
        ensure(
            note["kind"] == "start" && note["id"] == id && note["index"] == index,
            "Action start identity mismatch",
        )?;
        let start = ledger::action(index);
        starts.insert(id.clone(), start);
        let ack = tokio::time::timeout(Duration::from_secs(5), actor.execute(state, &action))
            .await
            .map_err(|_| "Native operation timeout")??;
        bridge
            .send(ack)
            .map_err(|_| "Action acknowledgement failed")?;
        let remaining = 5_000_000_000u64.saturating_sub(ledger::elapsed().saturating_sub(start));
        let result = tokio::time::timeout(Duration::from_nanos(remaining), bridge.recv::<Value>())
            .await
            .map_err(|_| "Native action endpoint timeout")?
            .map_err(|_| "Action evaluator ended")?;
        let _ = bridge.send(());
        ensure(
            result["ok"] == true && result["id"] == id && result["index"] == index,
            "Action endpoint failed",
        )?;
        actor.accepted(state, &action, &result)?;
        ledger::emit(ledger::event::ACTION_END, 0, 0, index);
        completed.push(result);
    }
    Err("Schedule has no terminal close".into())
}
