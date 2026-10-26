//! Channel occurrences reuse the accepted member harness and passive captures.
use super::*;
use directory::{ALPHA, BETA, GAMMA, REMOTE};
use inputs::channels::fixture as directory;
mod admission;
mod props;
mod retirement;
mod selection;

pub(super) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("app-home-channel-dispatch" | "app-home-channel-dispatch-narrow")
        )
}
pub(super) fn configure(raw: &mut Value) {
    raw["blocks"] = json!([
        {"type":"input","block_id":"pick","dispatch_action":true,"optional":true,"label":{"type":"plain_text","text":"Release channel"},"element":{"type":"channels_select","action_id":"value","initial_channel":ALPHA,"response_url_enabled":true,"placeholder":{"type":"plain_text","text":"Find a channel"}}},
        {"type":"input","block_id":"required","dispatch_action":true,"label":{"type":"plain_text","text":"Required channel"},"element":{"type":"channels_select","action_id":"value"}},
        {"type":"input","block_id":"quiet","optional":true,"label":{"type":"plain_text","text":"Quiet channel"},"element":{"type":"channels_select","action_id":"value","initial_channel":ALPHA}},
        {"type":"input","block_id":"multi","optional":true,"label":{"type":"plain_text","text":"Quiet channels"},"element":{"type":"multi_channels_select","action_id":"value","initial_channels":[ALPHA],"max_selected_items":2}},
        {"type":"input","block_id":"conversation","optional":true,"label":{"type":"plain_text","text":"Quiet conversation"},"element":{"type":"conversations_select","action_id":"value","initial_conversation":ALPHA,"filter":{"include":["public"]}}},
        {"type":"input","block_id":"conversations","optional":true,"label":{"type":"plain_text","text":"Quiet conversations"},"element":{"type":"multi_conversations_select","action_id":"value","initial_conversations":[ALPHA],"max_selected_items":2,"filter":{"include":["public"]}}},
        {"type":"input","block_id":"notes","optional":true,"label":{"type":"plain_text","text":"Release notes"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Sibling"}},
        {"type":"input","block_id":"day","optional":true,"label":{"type":"plain_text","text":"Review date"},"element":{"type":"datepicker","action_id":"value"}},
        {"type":"actions","block_id":"buttons","elements":[{"type":"button","action_id":"send","text":{"type":"plain_text","text":"Send form"},"value":"send"}]}
    ]);
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
        _ => Err("Unknown channel dispatch case".into()),
    }
}
pub(super) async fn choose(
    s: Signal<ShellState>,
    d: &DesktopContext,
    block: &str,
    id: &str,
) -> Result<Value, String> {
    let opened = open(d, block).await?;
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
    for _ in 0..150 {
        let x = snapshot(&s.peek());
        let wire = &x["formState"]["values"][block]["value"];
        let committed = wire["selected_channel"] == id
            || wire["selected_conversation"] == id
            || ["selected_channels", "selected_conversations"]
                .iter()
                .any(|k| wire[*k].as_array().is_some_and(|a| a.contains(&json!(id))));
        let displayed = inputs::channels::display(&s.peek(), &key(block))
            .is_some_and(|rows| rows.iter().any(|r| r.0 == id));
        if committed && displayed {
            return Ok(json!({"open":opened,"choose":chosen,"block":block,"id":id}));
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("Channel choice did not commit {block}/{id}"))
}
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
        if snapshot(&s.peek())["formState"]["values"][block]["value"].get("selected_channel")
            == Some(&Value::Null)
        {
            return Ok(proof);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err("Channel clear did not commit explicit null".into())
}
