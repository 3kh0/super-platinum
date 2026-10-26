//! Multi-conversation lanes share the accepted member fixture's leases and captures.
use super::channel::choose;
use super::*;
use inputs::channels::conversation::fixture::{
    ADA, ALPHA, BETA, PRIVATE as GAMMA, REMOTE, REMOTE_USER,
};
use inputs::channels::fixture as directory;
mod admission;
mod props;
mod retirement;
mod selection;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-multi-conversation-dispatch"
                | "app-home-multi-conversation-dispatch-narrow")
        )
}
pub(super) fn configure(raw: &mut Value) {
    for (index, label) in [
        (0, "Release conversations"),
        (1, "Required conversations"),
        (2, "Quiet conversations"),
    ] {
        let element = &mut raw["blocks"][index]["element"];
        *element = json!({"type":"multi_conversations_select","action_id":"value","max_selected_items":2,"default_to_current_conversation":true,
            "placeholder":{"type":"plain_text","text":"Find conversations"}});
        if index != 1 {
            element["initial_conversations"] = json!([ALPHA]);
        }
        raw["blocks"][index]["label"]["text"] = json!(label);
    }
    raw["blocks"][3] = json!({"type":"input","block_id":"single","optional":true,
        "label":{"type":"plain_text","text":"Quiet single conversation"},
        "element":{"type":"conversations_select","action_id":"value","initial_conversation":ALPHA}});
    raw["blocks"][4] = json!({"type":"input","block_id":"channel","optional":true,
        "label":{"type":"plain_text","text":"Quiet channel"},
        "element":{"type":"channels_select","action_id":"value","initial_channel":ALPHA}});
    raw["blocks"][5] = json!({"type":"input","block_id":"multi","optional":true,
        "label":{"type":"plain_text","text":"Quiet channels"},
        "element":{"type":"multi_channels_select","action_id":"value","initial_channels":[ALPHA],"max_selected_items":2}});
}
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    match case().as_str() {
        "selection" => selection::run(s, d, n, st, cap).await,
        "props" => props::run(s, d, n, st, cap).await,
        "retirement" => retirement::run(s, d, n, st, cap).await,
        "admission" => admission::run(s, d, n, st, cap).await,
        _ => Err("Unknown multi-conversation dispatch case".into()),
    }
}
async fn selected(s: Signal<ShellState>, block: &str, expected: &[&str]) -> Result<(), String> {
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        let widget = x["multiConversationWidgets"]
            .as_array()
            .and_then(|rows| rows.iter().find(|w| w["block"] == block));
        if x["formState"]["values"][block]["value"]["selected_conversations"] == json!(expected)
            && widget.is_some_and(|w| w["visible"] == json!(expected))
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!(
        "Multi-conversation ordered selection did not commit {block}: {}",
        json!(expected)
    ))
}
async fn toggle(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
    expected: &[&str],
) -> Result<Value, String> {
    let x = snapshot(&s.peek());
    let opened = if x["pickerBlock"] == block && x["directory"]["query"]["current"] == true {
        Value::Null
    } else {
        open(d, block).await?
    };
    let selector = format!("#app-home [data-modal-users='{block}'] [data-entity-id='{id}']");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let chosen = capture::click(&selector).await?;
    selected(s, block, expected).await?;
    Ok(json!({"open":opened,"choose":chosen,"block":block,"id":id,"expected":expected}))
}
async fn remove(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
    expected: &[&str],
) -> Result<Value, String> {
    let selector = format!("#app-home [data-modal-users='{block}'] [data-remove-user='{id}']");
    capture::wait(
        d,
        &format!(
            "!!document.querySelector({} + ':not(:disabled)')",
            json!(selector)
        ),
    )
    .await?;
    let removed = capture::click(&selector).await?;
    selected(s, block, expected).await?;
    Ok(json!({"remove":removed,"block":block,"id":id,"expected":expected}))
}
async fn notes_received(s: Signal<ShellState>, value: &str) -> Result<Value, String> {
    let proof = notes(value).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"]["notes"]["value"]["value"] == value {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Multi-conversation sibling notes not acknowledged".into())
}
