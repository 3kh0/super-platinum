//! Synthetic disconnected messages for passive reading and edit refusal.
use serde_json::{Value, json};
use super_platinum_core::slack::models::Message;

pub(crate) const TABLE_TS: &str = "1810000000.000001";
pub(crate) const PLAIN_TS: &str = "1810000000.000002";
pub(crate) const UNSUPPORTED_TS: &str = "1810000000.000003";

pub(crate) fn block() -> Value {
    let mut rows = vec![json!([
        {"type":"raw_text","text":"Project"},
        {"type":"raw_text","text":"Notes"},
        {"type":"raw_text","text":"Amount"}
    ])];
    for index in 1..=12 {
        rows.push(json!([
            {"type":"raw_text","text":format!("Project {index:02}")},
            if index == 1 {
                json!({"type":"rich_text","elements":[
                    {"type":"rich_text_section","elements":[
                        {"type":"text","text":"Ready <literal>","style":{"bold":true,"italic":true,"underline":true,"strike":true,"code":true}},
                        {"type":"link","url":"https://example.invalid/notes","text":"Release notes"},
                        {"type":"emoji","unicode":"1f44d"}]},
                    {"type":"rich_text_quote","elements":[{"type":"text","text":"Reviewed"}]},
                    {"type":"rich_text_preformatted","elements":[{"type":"text","text":"a < b\nsecond line"}]},
                    {"type":"rich_text_list","style":"ordered","offset":2,"elements":[
                        {"type":"rich_text_section","elements":[{"type":"text","text":"Ship"}]}]}
                ]})
            } else { json!({"type":"raw_text","text":"A supplied note\nwith a second line"}) },
            {"type":"raw_number","value":index * 1000,"text":format!("${index},000.00")}
        ]));
    }
    json!({"type":"table","rows":rows,"column_settings":[
        {"is_wrapped":true},null,{"align":"right"}
    ]})
}

pub(crate) fn apply(state: &mut crate::state::ShellState) {
    let team = state.core.active_team.clone().unwrap();
    let channel = state.core.active_channel.clone().unwrap();
    let user = state.core.workspaces[&team].self_user_id.clone();
    let raw = vec![
        Message {
            ts: Some(TABLE_TS.into()),
            team: Some(team.clone()),
            channel: Some(channel.clone()),
            user: Some(user.clone()),
            text: Some("Table fallback must not duplicate".into()),
            blocks: vec![block()],
            ..Default::default()
        },
        Message {
            ts: Some(PLAIN_TS.into()),
            team: Some(team.clone()),
            channel: Some(channel.clone()),
            user: Some(user.clone()),
            text: Some("Ordinary editable message".into()),
            ..Default::default()
        },
        Message {
            ts: Some(UNSUPPORTED_TS.into()),
            team: Some(team.clone()),
            channel: Some(channel.clone()),
            user: Some(user),
            text: Some("Unsupported table fallback".into()),
            blocks: vec![json!({"type":"table","rows":[[{"type":"file","file_id":"F1"}]]})],
            ..Default::default()
        },
    ];
    let projected = raw
        .iter()
        .map(|m| crate::message_vm::message_vm(&state.core.workspaces[&team], m, &state.media))
        .collect();
    let bag = state
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .messages
        .entry(channel)
        .or_default();
    bag.messages.clear();
    for message in raw {
        bag.upsert(message);
    }
    state.install_fixture_messages(projected);
}
