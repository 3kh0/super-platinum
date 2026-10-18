//! About descriptions use the same safe typed mrkdwn pipeline as messages.
use super::*;

pub(super) fn nodes(s: &ShellState, key: Key, body: &str) -> Vec<crate::state::RichNode> {
    if !current(s, key) || body.trim().is_empty() {
        return Vec::new();
    }
    let h = s.app_home.about.surface.as_ref().unwrap();
    let Some(workspace) = s.core.workspaces.get(&h.owner.ctx.session.team_id) else {
        return Vec::new();
    };
    crate::blocks::mrkdwn_blocks(crate::blocks::BlockCtx::new(workspace, &s.media), body)
}

pub(super) fn render(
    state: Signal<ShellState>,
    key: Key,
    kind: &'static str,
    body: &str,
) -> Element {
    let (nodes, epoch) = {
        let s = state.read();
        (nodes(&s, key, body), s.media_epoch)
    };
    if nodes.is_empty() {
        return rsx! {};
    }
    // Both kind values are local constants, never app-provided text.
    let id = format!("app-about-description-{}-{}-{kind}", key.id, key.revision);
    let mount_id = id.clone();
    let body_id = format!("{id}-body");
    rsx! {
        div {
            key: "{id}", id: "{id}", class: "app-about-description",
            "data-about-visit": key.id, "data-about-revision": key.revision,
            onmounted: move |_| {
                let id = serde_json::to_string(&mount_id).unwrap();
                let _ = dioxus::document::eval(&format!("{}\ninstallAppAboutDescription({id});", include_str!("description.js")));
            },
            div {
                id: "{body_id}", class: "app-about-description-body", "data-about-body": "true", "data-expanded": "false",
                for node in nodes { {crate::view::rich::rich_node(&node, epoch, state)} }
            }
            button {
                class: "app-about-description-toggle", "data-about-toggle": "true", hidden: true,
                "aria-controls": "{body_id}", "aria-expanded": "false", "Show more"
            }
        }
    }
}
