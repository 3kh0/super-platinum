//! Eligibility for omitting message-container state from `blocks.actions`.
//!
//! Slack's captured M0Sk/getInitialBlockState visits section accessories,
//! actions elements, and input elements; JSK/ enumerates stateful types. The
//! renderer does not yet own editable message input state, so initial values
//! cannot stand in for a user's current choices. Keep such messages gated until
//! that ownership exists. This is not validation of the clicked button.

use crate::slack::models::Message;
use serde_json::Value;

mod inputs;
pub use inputs::MessageInputs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStateError {
    StatefulControl,
    UnsupportedStructure,
    MalformedStructure,
    ExcessiveStructure,
    UnknownField,
    InvalidSelection,
}

/// Prove that a supported message body needs no local input state.
///
/// Inspect the whole canonical body, including unselected and invisible
/// controls. Attachments are separate containers and deliberately excluded.
/// Call this again after resolving the canonical message at dispatch time.
pub fn stateless(message: &Message) -> Result<(), MessageStateError> {
    if message.blocks.len() > 100 {
        return Err(MessageStateError::ExcessiveStructure);
    }
    let mut remaining = 10_000;
    for block in &message.blocks {
        // Unknown/conditional containers may expand to controls that are not
        // represented in our renderer. They require explicit support first.
        match kind(block)? {
            "section" => {
                if let Some(accessory) = block.get("accessory") {
                    element(accessory)?;
                }
            }
            "actions" => {
                let elements = block
                    .get("elements")
                    .and_then(Value::as_array)
                    .ok_or(MessageStateError::MalformedStructure)?;
                for value in elements {
                    element(value)?;
                }
            }
            "input" => return Err(MessageStateError::StatefulControl),
            "header" | "context" | "divider" | "image" | "rich_text" | "video" | "file"
            | "markdown" | "table" => {}
            _ => return Err(MessageStateError::UnsupportedStructure),
        }
        scan(block, 0, &mut remaining)?;
    }
    Ok(())
}

fn kind(value: &Value) -> Result<&str, MessageStateError> {
    value
        .get("type")
        .and_then(Value::as_str)
        .filter(|kind| !kind.is_empty())
        .ok_or(MessageStateError::MalformedStructure)
}

fn element(value: &Value) -> Result<(), MessageStateError> {
    let kind = kind(value)?;
    if stateful(kind) {
        return Err(MessageStateError::StatefulControl);
    }
    match kind {
        "button" | "overflow" | "icon_button" | "feedback_buttons" | "image" => Ok(()),
        _ => Err(MessageStateError::UnsupportedStructure),
    }
}

fn stateful(kind: &str) -> bool {
    matches!(
        kind,
        "users_select"
            | "conversations_select"
            | "channels_select"
            | "multi_users_select"
            | "multi_conversations_select"
            | "multi_channels_select"
            | "multi_static_select"
            | "multi_external_select"
            | "external_select"
            | "static_select"
            | "datepicker"
            | "timepicker"
            | "plain_text_input"
            | "number_input"
            | "radio_buttons"
            | "checkboxes"
            | "password"
            | "rich_text_input"
            | "datetimepicker"
            | "url_text_input"
            | "email_text_input"
            | "file_input"
    )
}

fn scan(value: &Value, depth: usize, remaining: &mut usize) -> Result<(), MessageStateError> {
    if depth > 32 || *remaining == 0 {
        return Err(MessageStateError::ExcessiveStructure);
    }
    *remaining -= 1;
    match value {
        Value::Object(object) => {
            if object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(stateful)
            {
                return Err(MessageStateError::StatefulControl);
            }
            if object.contains_key("action_id") {
                element(value)?;
            }
            for child in object.values() {
                scan(child, depth + 1, remaining)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                scan(child, depth + 1, remaining)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(blocks: Value) -> Message {
        serde_json::from_value(json!({"ts":"1.0", "blocks":blocks})).unwrap()
    }

    #[test]
    fn ordinary_buttons_and_rich_content_need_no_input_state() {
        let message = message(json!([
            {"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
                {"type":"text","text":"static_select is ordinary prose"}]}]},
            {"type":"section","text":{"type":"mrkdwn","text":"Choose"},
             "accessory":{"type":"button","action_id":"go","text":{"type":"plain_text","text":"Go"}}},
            {"type":"actions","elements":[{"type":"button","action_id":"done"},{"type":"overflow","action_id":"more"}]}
        ]));
        assert_eq!(stateless(&message), Ok(()));
    }

    #[test]
    fn every_captured_stateful_type_blocks_all_three_container_positions() {
        let kinds = [
            "users_select",
            "conversations_select",
            "channels_select",
            "multi_users_select",
            "multi_conversations_select",
            "multi_channels_select",
            "multi_static_select",
            "multi_external_select",
            "external_select",
            "static_select",
            "datepicker",
            "timepicker",
            "plain_text_input",
            "number_input",
            "radio_buttons",
            "checkboxes",
            "password",
            "rich_text_input",
            "datetimepicker",
            "url_text_input",
            "email_text_input",
            "file_input",
        ];
        for kind in kinds {
            let control = json!({"type":kind,"action_id":"input","visible_to_user_ids":[]});
            for block in [
                json!({"type":"section","accessory":control}),
                json!({"type":"actions","elements":[control]}),
                json!({"type":"input","element":control}),
            ] {
                assert_eq!(
                    stateless(&message(json!([block]))),
                    Err(MessageStateError::StatefulControl),
                    "{kind}"
                );
            }
        }
    }

    #[test]
    fn initial_values_do_not_authorize_omitting_current_state() {
        assert_eq!(
            stateless(&message(json!([{"type":"actions","elements":[
                {"type":"button","action_id":"submit"},
                {"type":"static_select","action_id":"selection","initial_option":{"value":"a"}}
            ]}]))),
            Err(MessageStateError::StatefulControl)
        );
    }

    #[test]
    fn unknown_conditional_and_malformed_shapes_do_not_look_stateless() {
        for block in [
            json!({"type":"condition","cases":[]}),
            json!({"type":"new_container"}),
        ] {
            assert_eq!(
                stateless(&message(json!([block]))),
                Err(MessageStateError::UnsupportedStructure)
            );
        }
        for block in [
            json!({}),
            json!({"type":"actions"}),
            json!({"type":"section","accessory":null}),
        ] {
            assert_eq!(
                stateless(&message(json!([block]))),
                Err(MessageStateError::MalformedStructure)
            );
        }
        assert_eq!(
            stateless(&message(
                json!([{"type":"actions","elements":[{"type":"future_select","action_id":"x"}]}])
            )),
            Err(MessageStateError::UnsupportedStructure)
        );
    }

    #[test]
    fn nested_control_is_not_hidden_by_a_display_container() {
        assert_eq!(
            stateless(&message(json!([{"type":"table","rows":[[
                {"type":"plain_text_input","action_id":"nested"}
            ]]}]))),
            Err(MessageStateError::StatefulControl)
        );
    }

    #[test]
    fn attachment_state_belongs_to_its_own_container() {
        let mut message =
            message(json!([{ "type":"actions","elements":[{"type":"button","action_id":"x"}]}]));
        message.attachments = serde_json::from_value(
            json!([{"blocks":[{"type":"input","element":{"type":"plain_text_input"}}]}]),
        )
        .unwrap();
        assert_eq!(stateless(&message), Ok(()));
    }

    #[test]
    fn processing_is_bounded() {
        let mut value = json!({"type":"rich_text"});
        for _ in 0..35 {
            value = json!({"type":"rich_text","elements":[value]});
        }
        assert_eq!(
            stateless(&message(json!([value]))),
            Err(MessageStateError::ExcessiveStructure)
        );
        assert_eq!(
            stateless(&message(
                json!([{"type":"rich_text","elements": vec![Value::Null; 10_001]}])
            )),
            Err(MessageStateError::ExcessiveStructure)
        );
    }
}
