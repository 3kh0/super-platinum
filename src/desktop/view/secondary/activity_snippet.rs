//! Native ordinary mention styling only; existing accepted body/fallback rules.
use super::snippet::Preview;
use crate::state::ShellState;

pub(super) fn project(
    snapshot: &ShellState,
    workspace: Option<&super_platinum_core::state::Workspace>,
    item: &super_platinum_core::slack::models::ActivityItem,
) -> Preview {
    if let (Some(workspace), Some(message)) =
        (workspace, super::activity_hydrated_message(snapshot, item))
    {
        let vm = crate::message_vm::message_vm(workspace, message, &snapshot.media);
        let text = super::super::rich::message_plain_text(&vm);
        let body = text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim();
        if item.item.reaction.is_some() {
            let author = message
                .user
                .as_deref()
                .map(|user| {
                    if user == workspace.self_user_id {
                        "You".to_owned()
                    } else {
                        workspace.display_name(user)
                    }
                })
                .unwrap_or_else(|| "You".into());
            return Preview::plain(format!("{author}: {body}"));
        }
        if !body.is_empty() {
            let rich = (item.item.kind == "at_user")
                .then(|| super::snippet::first(crate::message_vm::preview::nodes(vm.body), body))
                .flatten();
            return Preview {
                text: body.to_owned(),
                rich,
            };
        }
        if item
            .channel()
            .zip(message.ts.as_deref())
            .is_some_and(|(c, t)| snapshot.core.activity.has_original_body_edit(c, t))
        {
            return Preview::plain(String::new());
        }
    }
    if let Some(text) = item
        .item
        .message
        .as_ref()
        .and_then(|message| message.extra.get("text"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        if let Some(workspace) = workspace {
            let message = super_platinum_core::slack::models::Message {
                text: Some(text.to_owned()),
                ..Default::default()
            };
            let vm = crate::message_vm::message_vm(workspace, &message, &snapshot.media);
            let rendered = super::super::rich::message_plain_text(&vm);
            return Preview::plain(rendered.lines().next().unwrap_or(&rendered).to_owned());
        }
        return Preview::plain(text.lines().next().unwrap_or(text).to_owned());
    }
    if let Some(reaction) = item.item.reaction.as_ref() {
        let name = if reaction.name.is_empty() {
            "reaction"
        } else {
            reaction.name.as_str()
        };
        let glyph = super_platinum_core::state::emoji_glyph(name);
        let who = reaction
            .user
            .as_deref()
            .map(|user| {
                workspace
                    .map(|ws| ws.display_name(user))
                    .unwrap_or_else(|| user.to_owned())
            })
            .unwrap_or_else(|| "Someone".into());
        return Preview::plain(format!("{who}: {glyph}"));
    }
    Preview::plain(String::new())
}
