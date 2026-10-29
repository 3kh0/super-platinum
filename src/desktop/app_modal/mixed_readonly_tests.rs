//! Actual controller plus existing typed display projection; no native capture.
use super::*;
use crate::{media::MediaRegistry, model::RichNode};
use serde_json::json;
use super_platinum_core::block_actions::form::FormBlock;

fn payload(number: bool) -> Value {
    let mut blocks = vec![
        json!({"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Adjust inventory","emoji":true}}),
        json!({"type":"section","block_id":"instructions","text":{"type":"plain_text","text":"Enter a signed adjustment."}}),
        json!({"type":"context","block_id":"note","elements":[{"type":"plain_text","text":"Keep the original name.","emoji":true}]}),
        json!({"type":"divider","block_id":"separator"}),
        json!({"type":"input","block_id":"name","label":{"type":"plain_text","text":"Name"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained"}}),
    ];
    if number {
        blocks.insert(2,json!({"type":"input","block_id":"adjustment","label":{"type":"plain_text","text":"Adjustment"},"element":{"type":"number_input","action_id":"value","initial_value":"-1","is_decimal_allowed":false}}));
    }
    json!({"id":"V_MIXED_READONLY","type":"modal","app_id":"A1","title":{"type":"plain_text","text":"Inventory"},"submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn deliver(shell: &mut ShellState, kind: &str, token: &str, view: Value) {
    let scope = scope(shell).unwrap();
    let event=RtEvent::Unknown(serde_json::from_value(json!({"type":kind,"view_id":view["id"],"app_id":"A1","view_type":"modal","client_token":token,"view":view})).unwrap());
    realtime(shell, &scope.team, scope.realtime_generation, &event);
}
fn literal(nodes: &[RichNode], expected: &str) -> bool {
    matches!(nodes,[RichNode::Text(text)] if text==expected)
}
#[test]
fn real_correlated_mixed_instructions_project_and_new_required_quantity_submits_only_input_rows() {
    let mut shell = ShellState::fixture(MediaRegistry::default());
    crate::app_home::fixture::install(&mut shell);
    let action = register_action(&mut shell, "A1").unwrap();
    assert!(action_scope_current(&shell, &action));
    let initial = payload(false);
    let displays: Vec<_> = initial["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] != "input")
        .cloned()
        .collect();
    deliver(&mut shell, "view_opened", &action.token, initial);
    assert!(!shell.app_modal.as_ref().unwrap().fixture);
    assert!(
        shell
            .app_modal
            .as_ref()
            .unwrap()
            .modal
            .integer_introduction_context_live()
    );
    deliver(&mut shell, "view_updated", &action.token, payload(true));
    let shown = presentation(&shell).unwrap();
    assert!(!shown.locked);
    assert!(shown.view_error.is_none());
    let form = shown.form.as_ref().unwrap();
    assert!(form.unsupported.is_empty());
    let retained: Vec<_> = form
        .blocks
        .iter()
        .filter_map(|b| match b {
            FormBlock::Display(v) => Some(v.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(retained, displays);
    let workspace = &shell.core.workspaces[shell.core.active_team.as_ref().unwrap()];
    let rendered: Vec<_> = retained
        .iter()
        .map(|b| {
            crate::blocks::block_nodes(crate::blocks::BlockCtx::new(workspace, &shell.media), b)
        })
        .collect();
    assert!(
        matches!(rendered[0].as_slice(),[RichNode::Header(text)] if literal(text,"Adjust inventory"))
    );
    assert!(
        matches!(rendered[1].as_slice(),[RichNode::Section{text,fields,accessory}] if fields.is_empty()&&accessory.is_none()&&matches!(text.as_slice(),[RichNode::Paragraph(line)] if literal(line,"Enter a signed adjustment.")))
    );
    assert!(
        matches!(rendered[2].as_slice(),[RichNode::Context(text)] if literal(text,"Keep the original name."))
    );
    assert!(matches!(rendered[3].as_slice(), [RichNode::Divider]));
    assert_eq!(
        requests::prepare_submission(&shell).unwrap(),
        json!({"values":{"name":{"value":{"type":"plain_text_input","value":"Retained"}},"adjustment":{"value":{"type":"number_input","value":"-1"}}}})
    );
    let key = InputKey {
        block_id: "adjustment".into(),
        action_id: "value".into(),
    };
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key, InputValue::Number(None))
        .unwrap();
    assert!(matches!(
        requests::prepare_submission(&shell),
        Err(FormError::Validation(_))
    ));
    shell
        .app_modal
        .as_mut()
        .unwrap()
        .current_form_mut()
        .unwrap()
        .set_value(&key, InputValue::Number(Some("-2".into())))
        .unwrap();
    let (wire, _, _) = requests::accept_integer_submission_for_test(&mut shell).unwrap();
    assert_eq!(
        wire,
        json!({"values":{"name":{"value":{"type":"plain_text_input","value":"Retained"}},"adjustment":{"value":{"type":"number_input","value":"-2"}}}})
    );
    assert!(requests::accept_integer_submission_for_test(&mut shell).is_none());
    // The test adapter accepts serial custody without executing its Transport.
    assert!(presentation(&shell).unwrap().busy);
}
