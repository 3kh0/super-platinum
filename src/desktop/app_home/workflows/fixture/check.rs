//! Actual mounted workflow controls with the closed fixture executor only.
use super::*;
use std::time::Duration;
mod authentication;

type Desktop = dioxus::desktop::DesktopContext;

pub(crate) fn start(state: Signal<ShellState>, desktop: Desktop) {
    if std::env::var("SUPER_PLATINUM_HOME_WORKFLOW_CHECK").as_deref() != Ok("1") {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default();
    let base = name.strip_suffix("-narrow").unwrap_or(&name);
    if !matches!(
        base,
        "app-home-workflow"
            | "app-home-workflow-held"
            | "app-home-workflow-unconfirmed"
            | "app-home-workflow-rate-limit"
            | "app-home-workflow-auth"
            | "app-home-workflow-auth-held"
            | "app-home-workflow-auth-unconfirmed"
            | "app-home-workflow-auth-rate-limit"
    ) || state.peek().app_home.workflow_fixture.is_none()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        let result =
            tokio::time::timeout(Duration::from_secs(90), run(state, &desktop, &name)).await;
        let receipt = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"error":error,"model":snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Workflow native fixture timed out","model":snapshot(&state.peek())})
            }
        };
        eprintln!("home-workflow-check: {receipt}");
    });
}
async fn dom(operation: &str, args: Value) -> Result<Value, String> {
    let script = format!(
        "{}\nconst receipt=window.homeWorkflowCheck({},{});dioxus.send(receipt);await dioxus.recv();",
        include_str!("check.js"),
        json!(operation),
        args
    );
    let mut eval = document::eval(&script);
    let received = tokio::time::timeout(Duration::from_secs(3), eval.recv::<Value>()).await;
    let released = eval.send(());
    let value = received
        .map_err(|_| "Workflow native receipt timeout")?
        .map_err(|_| "Workflow native receipt missing")?;
    if value["ok"] != true {
        return Err(format!("Workflow native {operation}: {value}"));
    }
    released.map_err(|_| "Workflow native acknowledgment failed")?;
    Ok(value)
}
async fn action(
    operation: &str,
    generation: Option<&str>,
    mut args: Value,
) -> Result<Value, String> {
    args["token"] = json!(format!("workflow-check-{}", next()));
    args["generation"] = json!(generation);
    // Deliberately no retry: a missing reply cannot prove the click was absent.
    dom(operation, args).await
}
async fn wait(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&Value, &Value) -> bool,
) -> Result<Value, String> {
    let mut last = Value::Null;
    for _ in 0..400 {
        if let Ok(native) = dom("read", json!({})).await {
            let model = snapshot(&state.peek());
            if test(&model, &native) {
                return Ok(native);
            }
            last = json!({"model":model,"native":native});
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("Workflow wait for {label}: {last}"))
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &Desktop,
    name: &str,
    stage: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let native = dom("read", json!({})).await?;
    if native["dialog"].is_object() && native["dialog"]["inside"] != true {
        return Err("Workflow dialog clipped".into());
    }
    let directory =
        std::env::var("SUPER_PLATINUM_UI_CAPTURE_DIR").unwrap_or_else(|_| "tmp/agent-ui".into());
    let path = std::path::Path::new(&directory).join(format!("{name}-{stage}.png"));
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into()), desktop).await?;
    stages.push(json!({"stage":stage,"path":path,"width":width,"height":height,"native":native,"model":snapshot(&state.peek())}));
    Ok(())
}
fn starts(model: &Value) -> usize {
    model["ledger"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| {
                    r["path"] == "/api/workflows.triggers.trip"
                        && r["body"].get("run_precheck").is_none()
                })
                .count()
        })
        .unwrap_or(0)
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &Desktop,
    name: &str,
) -> Result<Value, String> {
    if name.contains("-auth") {
        return authentication::run(state, desktop, name).await;
    }
    let mut stages = Vec::new();
    wait(state, "current Home", |m, n| {
        m["current"] == true && n["home"] == true && n["dialog"].is_null()
    })
    .await?;
    capture(state, desktop, name, "home", &mut stages).await?;
    action("open", None, json!({})).await?;
    let native = wait(state, "owned mounted review", |m, n| {
        m["dialog"]["id"].is_string() && m["dialog"]["id"] == n["dialog"]["generation"]
    })
    .await?;
    let generation = native["dialog"]["generation"].as_str().unwrap().to_owned();
    action("title", Some(&generation), json!({"value":"Release 7"})).await?;
    wait(state, "accepted title", |m, n| {
        m["dialog"]["title"] == "Release 7" && n["dialog"]["title"] == "Release 7"
    })
    .await?;
    for (label, field, expected) in [
        ("Reviewer", "person", "UWORKADA1"),
        ("Conversation", "destination", "DWORKFLOW"),
    ] {
        action("field", Some(&generation), json!({"label":label})).await?;
        wait(state, "mounted entity query", |_, n| {
            n["dialog"]["picker"].is_object()
        })
        .await?;
        action("query", Some(&generation), json!({"value":"Ada"})).await?;
        wait(state, "current ranked Ada result", |_, n| {
            n["dialog"]["picker"]["query"] == "Ada"
                && n["dialog"]["picker"]["results"]
                    .as_array()
                    .is_some_and(|r| {
                        r.iter()
                            .filter(|v| {
                                v["label"] == "Ada" && v["disabled"] == false && v["inside"] == true
                            })
                            .count()
                            == 1
                    })
        })
        .await?;
        capture(state, desktop, name, field, &mut stages).await?;
        action("choose", Some(&generation), json!({"label":"Ada"})).await?;
        wait(state, "exact accepted entity identity", |m, n| {
            m["dialog"][field] == expected && n["dialog"]["picker"].is_null()
        })
        .await?;
    }
    wait(state, "Start admission", |m, n| {
        m["dialog"]["entitiesReady"] == true
            && m["dialog"]["ready"] == true
            && n["dialog"]["startEnabled"] == true
    })
    .await?;
    capture(state, desktop, name, "reviewed", &mut stages).await?;
    action("start", Some(&generation), json!({})).await?;
    if name.contains("-held") {
        wait(state, "held actual request", |m, _| {
            m["held"] == true && starts(m) == 1
        })
        .await?;
        capture(state, desktop, name, "held", &mut stages).await?;
        action("cancel", Some(&generation), json!({})).await?;
        wait(state, "dismissal retains sent attempt", |m, n| {
            n["dialog"].is_null()
                && m["attempts"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|v| v["cancelled"] == true && v["phase"] == "sent")
                })
        })
        .await?;
        if !release(&mut state.write()) {
            return Err("Held reply missing".into());
        }
        wait(state, "late settlement without reopening", |m, n| {
            m["held"] == false
                && m["attempts"].as_array().is_some_and(Vec::is_empty)
                && n["dialog"].is_null()
        })
        .await?;
    } else if name.contains("-unconfirmed") {
        wait(state, "retained uncertain delivery", |m, n| {
            m["attempts"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v["phase"] == "uncertain"))
                && n["dialog"]["startEnabled"] == false
        })
        .await?;
    } else if name.contains("-rate-limit") {
        wait(state, "actual cooldown", |m, _| {
            m["cooling"] == true && starts(m) == 1
        })
        .await?;
        capture(state, desktop, name, "cooldown", &mut stages).await?;
        wait(state, "cooldown expiry with retained input", |m, n| {
            m["cooling"] == false
                && m["dialog"]["title"] == "Release 7"
                && n["dialog"]["startEnabled"] == true
        })
        .await?;
        action("start", Some(&generation), json!({})).await?;
        wait(state, "explicit retry accepted", |m, n| {
            m["started"] == true
                && m["attempts"].as_array().is_some_and(Vec::is_empty)
                && n["dialog"].is_null()
        })
        .await?;
    } else {
        wait(state, "accepted result closes", |m, n| {
            m["started"] == true
                && m["attempts"].as_array().is_some_and(Vec::is_empty)
                && n["dialog"].is_null()
        })
        .await?;
    }
    let model = snapshot(&state.peek());
    let expected = if name.contains("-rate-limit") { 2 } else { 1 };
    if starts(&model) != expected {
        return Err("Unexpected workflow dispatch count".into());
    }
    capture(state, desktop, name, "settled", &mut stages).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scope":"closed native input/entity/Start workflow; no live Slack or authentication claim","stages":stages,"final":model}),
    )
}
