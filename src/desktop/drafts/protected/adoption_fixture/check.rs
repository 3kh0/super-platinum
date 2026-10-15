//! Real consent/adoption controls, isolated workers, and non-ticket DOM evidence.
use super as fixture;
use crate::state::{ComposerTarget, MainView, ShellState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{sync::atomic::Ordering, time::Duration};
#[path = "check_helpers.rs"]
mod helpers;
#[path = "check_scenarios.rs"]
mod scenarios;
#[path = "terminal.rs"]
mod terminal;
use helpers::*;
const FINAL: &str =
    "Later local input \u{2014} \u{65e5}\u{672c}\u{8a9e} \u{1f680} e\u{301} \u{2014} after retry";

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if !fixture::enabled() {
        return;
    }
    dioxus::core::spawn_forever(Box::pin(async move {
        let value = match tokio::time::timeout(Duration::from_secs(200), run(state, &desktop)).await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => json!({"ok":false,"error":e}),
            Err(_) => json!({"ok":false,"error":"Adoption checker timed out"}),
        };
        if let Err(error) = terminal::emit(value).await {
            // The runner requires a complete terminal line; this diagnostic
            // cannot substitute success after a failed or partial log write.
            eprintln!("protected-adoption-log-error: {error}");
        }
    }));
}
async fn ui_wait(dialog: bool) -> Result<(), String> {
    for _ in 0..150 {
        let value=eval(format!("const d=document.getElementById('protected-enrollment-dialog'),s=document.getElementById('protected-adoption-status'),r=document.getElementById('protected-adoption-reload');dioxus.send({{ok:true,ready:{}}});await dioxus.recv();",if dialog{"!!d&&!!document.getElementById('protected-enrollment-submit')"}else{"!d&&!!s&&!!r&&!r.disabled&&!document.getElementById('protected-adoption-cancel')"})).await?;
        if value["ready"] == true {
            return frames().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Adoption surface did not settle".into())
}
async fn click(control: &str, text: &str) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await protectedAdoptionClick({}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("check_dom.js"),json!({"control":control,"text":text}))).await
}
async fn surface(page: &str) -> Result<Value, String> {
    eval(format!("{};try{{dioxus.send(await protectedAdoptionSurface({}));}}catch(error){{dioxus.send({{ok:false,error:String(error)}});}}await dioxus.recv();",include_str!("check_dom.js"),json!({"page":page}))).await
}
async fn writable(state: Signal<ShellState>, b: &fixture::Binding) -> Result<Value, String> {
    for _ in 0..350 {
        let v = native(state, b.target(), "read", None).await?;
        if v["readOnly"] == false && v["disabled"] == false && v["admission"].is_null() {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("Original native readOnly was not restored".into())
}
async fn type_text(
    state: Signal<ShellState>,
    b: &fixture::Binding,
    text: &str,
    expected_error: Option<&str>,
) -> Result<Value, String> {
    let routed = b.routed.load(Ordering::Acquire);
    let observed = native(state, b.target(), "type", Some(text)).await?;
    for _ in 0..150 {
        if b.routed.load(Ordering::Acquire) > routed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ensure(
        b.routed.load(Ordering::Acquire) == routed + 1 && b.refused.load(Ordering::Acquire) == 0,
        "Input did not use exactly one protected route",
    )?;
    let result = fixture::flush_inputs(b).await;
    match expected_error {
        Some(error) => ensure(
            result.as_ref().err().map(String::as_str) == Some(error),
            "Wrong real input failure",
        )?,
        None => result?,
    }
    Ok(json!({"observed":observed,"flushError":expected_error}))
}
struct Evidence<'a> {
    state: Signal<ShellState>,
    desktop: &'a dioxus::desktop::DesktopContext,
    b: &'a fixture::Binding,
    captures: serde_json::Map<String, Value>,
    snapshots: Vec<Value>,
    details: serde_json::Map<String, Value>,
}
impl Evidence<'_> {
    async fn capture(&mut self, stage: &str) -> Result<Value, String> {
        let mut pages = Vec::new();
        for page in if stage == "dialog" {
            vec!["channel", "thread"]
        } else {
            vec!["status"]
        } {
            pages.push(surface(page).await?);
            let name = if stage == "dialog" {
                format!("dialog-{page}")
            } else {
                stage.into()
            };
            let path = self.b.capture_path(&name).await?;
            eprintln!(
                "protected-adoption-stage: {}",
                json!({"stage":name,"edge":"before"})
            );
            let (path, _, _) = crate::agent::capture::window(
                Some(path.to_string_lossy().into_owned()),
                self.desktop,
            )
            .await?;
            self.captures
                .insert(name.clone(), json!(path.to_string_lossy()));
            eprintln!(
                "protected-adoption-stage: {}",
                json!({"stage":name,"edge":"after"})
            );
        }
        let field = native(self.state, self.b.target(), "read", None).await?;
        // Plain read checks geometry without replacing the original admission ticket.
        let composer = if stage == "dialog" {
            Value::Null
        } else {
            dom(self.b.target(), "read", None).await?
        };
        let projection = fixture::projection(&self.state.peek(), self.b);
        ensure(
            projection["sessionPresent"] == false
                && projection["transportPresent"] == false
                && projection["cachePresent"] == false,
            "Network/cache capability in fixture",
        )?;
        ensure(
            projection["isolated"] == json!(stage != "away"),
            "Unexpected selected owner",
        )?;
        let record = json!({"stage":stage,"pages":pages,"native":field,"composer":composer,"durable":fixture::durable(self.b).await?,"projection":projection});
        self.snapshots.push(record.clone());
        Ok(record)
    }
    async fn button(&mut self, name: &str, control: &str, text: &str) -> Result<(), String> {
        let kind = match control {
            "open" => "open-click",
            "submit" => "consent-click",
            "reload" => "reload-click",
            "adopt" => "adopt-click",
            "cancel" => "adoption-cancel-click",
            "check" => "adoption-check-click",
            "retry" => "retry-input-click",
            "resume" => "resume-click",
            _ => return Err("Unknown adoption control".into()),
        };
        let count = || {
            self.b
                .actions
                .lock()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] == kind)
                .count()
        };
        let before = count();
        let clicked = click(control, text).await?;
        // DOM dispatch returning is not acknowledgment by the Rust handler.
        // In particular, Check status can still expose the preceding result.
        // The actual handler clears that result before recording its event.
        for _ in 0..150 {
            let now = count();
            ensure(now <= before + 1, "A control dispatched more than once")?;
            if now == before + 1 {
                frames().await?;
                self.details.insert(name.into(), clicked);
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(format!(
            "Actual {kind} handler did not acknowledge the click"
        ))
    }
    async fn result(&mut self, name: &str) -> Result<Value, String> {
        let value = self.b.wait_result().await?;
        ui_wait(false).await?;
        self.details.insert(name.into(), value.clone());
        self.details.insert(
            format!("{name}Writable"),
            writable(self.state, self.b).await?,
        );
        Ok(value)
    }
    async fn attempt(&mut self, name: &str, gate: &str) -> Result<Value, String> {
        self.details.insert(
            format!("{name}Before"),
            native(self.state, self.b.target(), "read", None).await?,
        );
        self.b.arm_gate(gate)?;
        self.button(&format!("{name}Click"), "adopt", "Continue editing")
            .await?;
        self.b.wait_gate(gate).await?;
        self.capture(name).await
    }
}
async fn run(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let b = fixture::install(state).await?;
    fixture::wait_runner(&b).await?;
    let initial = home(state, b.target()).await?;
    native(state, b.target(), "selection", None).await?;
    ui_wait(false).await?;
    let mut e = Evidence {
        state,
        desktop,
        b: &b,
        captures: Default::default(),
        snapshots: vec![],
        details: Default::default(),
    };
    e.details.insert("nativeInitial".into(), initial);
    e.capture("initial").await?;
    e.button("open", "open", "Review draft").await?;
    ui_wait(true).await?;
    e.capture("dialog").await?;
    e.button("consent", "submit", "Enable recovery").await?;
    let enrolled = e.result("enrollmentResult").await?;
    ensure(
        enrolled["kind"] == "enrolled" && enrolled["current"] == true,
        "Actual enrollment failed",
    )?;
    e.capture("enrollment").await?;
    e.button("reload", "reload", "Reload saved drafts").await?;
    ui_wait(false).await?;
    e.capture("reloaded").await?;
    scenarios::run(&mut e).await?;
    let final_inventory = fixture::durable(&b).await?;
    let final_projection = fixture::projection(&state.peek(), &b);
    Ok(
        json!({"ok":true,"fixture":std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_|"Missing fixture alias")?,"scenario":b.scenario,"phase":"exercise","nonce":b.nonce,"pid":std::process::id(),"isolatedRoot":b.root.to_string_lossy(),"target":label(b.target()),"viewport":e.snapshots[0]["pages"][0]["viewport"],"captures":e.captures,"snapshots":e.snapshots,"details":e.details,"finalInventory":final_inventory,"finalProjection":final_projection,"evidenceScope":"isolated protected enrollment/native route adoption; no clear or network capability"}),
    )
}
