//! Conversation occurrences reuse the accepted member harness and passive captures.
use super::*;
use inputs::channels::conversation::fixture::{ADA, ALPHA, BETA, PRIVATE, REMOTE, REMOTE_USER};
use inputs::channels::fixture as directory;
mod admission;
mod props;
mod retirement;
mod selection;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-conversation-dispatch" | "app-home-conversation-dispatch-narrow")
        )
}
pub(super) fn configure(raw: &mut Value) {
    for (index, label) in [
        (0, "Release conversation"),
        (1, "Required conversation"),
        (2, "Quiet conversation"),
    ] {
        let element = &mut raw["blocks"][index]["element"];
        *element = json!({"type":"conversations_select","action_id":"value","default_to_current_conversation":true,"placeholder":{"type":"plain_text","text":"Find a conversation"}});
        if index == 0 {
            element["response_url_enabled"] = json!(true);
        }
        if index == 2 || (index == 0 && case() != "selection") {
            element["initial_conversation"] = json!(ALPHA);
        }
        raw["blocks"][index]["label"]["text"] = json!(label);
    }
    raw["blocks"][5]["element"]
        .as_object_mut()
        .unwrap()
        .remove("filter");
    raw["blocks"][4] = json!({"type":"input","block_id":"channel","optional":true,"label":{"type":"plain_text","text":"Quiet channel"},"element":{"type":"channels_select","action_id":"value","initial_channel":ALPHA}});
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
        _ => Err("Unknown conversation dispatch case".into()),
    }
}
use super::channel::choose;
async fn clear(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
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
    let proof = capture::click(&selector).await?;
    for _ in 0..150 {
        if snapshot(&s.peek())["formState"]["values"][block]["value"].get("selected_conversation")
            == Some(&Value::Null)
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Conversation clear did not commit explicit null".into())
}
