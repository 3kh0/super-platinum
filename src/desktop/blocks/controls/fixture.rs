//! Disconnected native capture of server-supplied initial control values.

pub(crate) fn apply(state: &mut crate::state::ShellState) {
    use serde_json::json;
    let groups = [
        (
            "Release status and reviewers",
            json!([
                {"type":"static_select","placeholder":{"type":"plain_text","text":"Choose status"},"initial_option":{"text":{"type":"plain_text","text":"Ready to ship"},"value":"ready"}},
                {"type":"multi_users_select","initial_users":["U1","U2"]}
            ]),
        ),
        (
            "Selected teams and conversation",
            json!([
                {"type":"multi_external_select","initial_options":[{"text":{"type":"plain_text","text":"Design"},"value":"design"},{"text":{"type":"plain_text","text":"Engineering"},"value":"eng"}]},
                {"type":"conversations_select","initial_conversation":"C2"}
            ]),
        ),
        (
            "Launch date and time",
            json!([
                {"type":"datepicker","initial_date":"2026-10-13"},
                {"type":"timepicker","initial_time":"14:30","timezone":"America/Chicago"}
            ]),
        ),
        (
            "Selected checks",
            json!([
                {"type":"checkboxes","initial_options":[{"text":{"type":"plain_text","text":"Tests passed"},"value":"tested"},{"text":{"type":"plain_text","text":"Reviewed"},"value":"reviewed"}]},
                {"type":"radio_buttons","initial_option":{"text":{"type":"plain_text","text":"Stable"},"value":"stable"}}
            ]),
        ),
        (
            "Supplied text and empty selection",
            json!([
                {"type":"plain_text_input","initial_value":"Release notes are ready"},
                {"type":"multi_external_select","placeholder":{"type":"plain_text","text":"Choose projects"}}
            ]),
        ),
    ];
    let workspace = &state.core.workspaces["T1"];
    let messages = groups
        .into_iter()
        .enumerate()
        .map(|(index, (title, elements))| {
            let message = serde_json::from_value(json!({
                "ts":format!("1719800{:03}.000000",index * 60), "user":"U1",
                "blocks":[{"type":"section","text":{"type":"plain_text","text":title}},
                           {"type":"actions","elements":elements}]
            }))
            .expect("initial control values fixture");
            crate::message_vm::message_vm(workspace, &message, &state.media)
        })
        .collect();
    state.install_fixture_messages(messages);
}
