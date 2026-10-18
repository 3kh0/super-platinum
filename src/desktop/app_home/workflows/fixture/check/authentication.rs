//! Existing-account selection through mounted production controls.
use super::*;
fn selected(m: &Value) -> bool {
    m["dialog"]["connections"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["tokens"].as_array().is_some_and(|tokens| {
                tokens
                    .iter()
                    .any(|t| t["id"] == "xt2" && t["default"] == true)
            })
        })
    })
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &Desktop,
    name: &str,
) -> Result<Value, String> {
    let mut stages = Vec::new();
    wait(state, "current Home", |m, n| {
        m["current"] == true && n["home"] == true && n["dialog"].is_null()
    })
    .await?;
    capture(state, desktop, name, "home", &mut stages).await?;
    action("open", None, json!({})).await?;
    let n = wait(state, "existing-account picker", |m, n| {
        m["dialog"]["id"] == n["dialog"]["generation"] && n["dialog"]["account"]["value"] == "xt1"
    })
    .await?;
    let generation = n["dialog"]["generation"]
        .as_str()
        .ok_or("missing generation")?
        .to_owned();
    capture(state, desktop, name, "accounts", &mut stages).await?;
    action("account", Some(&generation), json!({"value":"xt2"})).await?;
    if name.contains("-held") {
        wait(state, "held mapping", |m, _| {
            m["held"] == true && m["authentication"]["mappings"] == 1
        })
        .await?;
        capture(state, desktop, name, "held", &mut stages).await?;
        action("cancel", Some(&generation), json!({})).await?;
        wait(state, "mapping custody after cancel", |m, n| {
            n["dialog"].is_null()
                && m["attempts"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|a| a["cancelled"] == true && a["phase"] == "sent")
                })
        })
        .await?;
        if !release(&mut state.write()) {
            return Err("held mapping missing".into());
        }
        wait(state, "late mapping does not reopen", |m, n| {
            m["attempts"].as_array().is_some_and(Vec::is_empty) && n["dialog"].is_null()
        })
        .await?;
    } else if name.contains("-unconfirmed") {
        wait(state, "mapping remains unconfirmed", |m, n| {
            m["attempts"]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["phase"] == "uncertain"))
                && !selected(m)
                && n["dialog"]["startEnabled"] == false
        })
        .await?;
    } else {
        if name.contains("-rate-limit") {
            wait(state, "mapping cooldown", |m, _| {
                m["cooling"] == true && m["authentication"]["mappings"] == 1 && !selected(m)
            })
            .await?;
            capture(state, desktop, name, "cooldown", &mut stages).await?;
            wait(state, "explicit selection retry available", |m, n| {
                m["cooling"] == false && n["dialog"]["account"]["disabled"] == false
            })
            .await?;
            action("account", Some(&generation), json!({"value":"xt2"})).await?;
        }
        wait(state, "exact mapping acknowledged", |m, n| {
            selected(m)
                && n["dialog"]["account"]["value"] == "xt2"
                && n["dialog"]["startEnabled"] == true
        })
        .await?;
        capture(state, desktop, name, "mapped", &mut stages).await?;
        action("start", Some(&generation), json!({})).await?;
        wait(state, "accepted workflow", |m, n| {
            m["started"] == true
                && n["dialog"].is_null()
                && m["attempts"].as_array().is_some_and(Vec::is_empty)
        })
        .await?;
    }
    let model = snapshot(&state.peek());
    let expected_mappings = if name.contains("-rate-limit") { 2 } else { 1 };
    let expected_starts = if name.contains("-held") || name.contains("-unconfirmed") {
        0
    } else {
        1
    };
    if model["authentication"]["mappings"] != expected_mappings || starts(&model) != expected_starts
    {
        return Err("unexpected authentication dispatch count".into());
    }
    if model["ledger"].as_array().is_some_and(|rows| {
        rows.iter()
            .any(|r| r["path"] == "/api/apps.auth.external.start")
    }) {
        return Err("unexpected OAuth launch request".into());
    }
    capture(state, desktop, name, "settled", &mut stages).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scope":"actual mounted existing-account mapping and Start under closed fixture; no OAuth/browser/live Slack acceptance","stages":stages,"final":model}),
    )
}
