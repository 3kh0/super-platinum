use super::*;
use crate::block_actions::form::FormBlock;
use serde_json::json;

fn key(id: &str) -> InputKey {
    InputKey {
        block_id: id.into(),
        action_id: "value".into(),
    }
}
fn displays() -> Vec<Value> {
    vec![
        json!({"type":"header","block_id":"heading","text":{"type":"plain_text","text":"Adjust inventory","emoji":true}}),
        json!({"type":"section","block_id":"instructions","text":{"type":"plain_text","text":"Enter a signed adjustment."}}),
        json!({"type":"context","block_id":"note","elements":[{"type":"plain_text","text":"Keep the original name.","emoji":true}]}),
        json!({"type":"divider","block_id":"separator"}),
    ]
}
fn text_input() -> Value {
    json!({"type":"input","block_id":"name","label":{"type":"plain_text","text":"Name"},"element":{"type":"plain_text_input","action_id":"value","initial_value":"Retained"}})
}
fn quantity(id: &str) -> Value {
    json!({"type":"input","block_id":id,"label":{"type":"plain_text","text":"Adjustment"},"element":{"type":"number_input","action_id":"value","initial_value":"-1","is_decimal_allowed":false}})
}
fn view(blocks: Vec<Value>) -> Value {
    json!({"id":"V_MIXED","type":"modal","app_id":"A1","title":{"type":"plain_text","text":"Inventory"},"submit":{"type":"plain_text","text":"Save"},"blocks":blocks})
}
fn frame(kind: &str, payload: Value) -> RawEvent {
    serde_json::from_value(json!({"type":kind,"view_id":"V_MIXED","app_id":"A1","view_type":"modal","client_token":"owned","view":payload})).unwrap()
}
fn root() -> (ModalState, ModalForm) {
    let scope = ModalScope {
        account_epoch: 1,
        team: "T1".into(),
        realtime_generation: 7,
    };
    let mut state = ModalState::new(scope.clone());
    assert!(state.enqueue(&scope, "owned".into(), "A1".into(), 0, 60_000));
    let mut blocks = displays();
    blocks.push(text_input());
    assert_eq!(
        state.apply_realtime_integer_context(&scope, &frame("view_opened", view(blocks)), 1),
        ModalChange::Opened
    );
    assert!(state.integer_introduction_context_live());
    let form = state.parse_integer_root_form().unwrap();
    (state, form)
}
#[test]
fn instructions_survive_new_quantity_and_only_inputs_submit_while_display_key_reuse_refuses() {
    let (mut state, mut form) = root();
    let mut blocks = displays();
    blocks.insert(2, text_input());
    blocks.push(quantity("adjustment"));
    let scope = state.scope().clone();
    assert_eq!(
        state.apply_realtime_integer_context(&scope, &frame("view_updated", view(blocks)), 2),
        ModalChange::Updated
    );
    state.reconcile_integer_form(&mut form).unwrap();
    let retained: Vec<_> = form
        .blocks
        .iter()
        .filter_map(|block| match block {
            FormBlock::Display(value) => Some(value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(retained, displays());
    assert!(form.unsupported.is_empty());
    assert_eq!(
        form.submission().unwrap(),
        json!({"values":{"name":{"value":{"type":"plain_text_input","value":"Retained"}},"adjustment":{"value":{"type":"number_input","value":"-1"}}}})
    );
    form.set_value(&key("adjustment"), InputValue::Number(Some("23".into())))
        .unwrap();
    assert_eq!(
        form.submission().unwrap()["values"]["adjustment"]["value"]["value"],
        "23"
    );
    form.set_value(&key("adjustment"), InputValue::Number(None))
        .unwrap();
    assert!(matches!(form.submission(), Err(FormError::Validation(_))));
    form.set_value(&key("adjustment"), InputValue::Number(Some("-2".into())))
        .unwrap();
    assert!(state.consume_integer_submission(&form));
    assert!(form.submission().is_err());

    let (mut state, mut form) = root();
    let scope = state.scope().clone();
    let mut blocks = displays();
    blocks[0] = quantity("heading");
    blocks.push(text_input());
    state.apply_realtime_integer_context(&scope, &frame("view_updated", view(blocks)), 2);
    state.reconcile_integer_form(&mut form).unwrap();
    assert_eq!(
        form.fields[&key("heading")].value,
        InputValue::Number(Some("-1".into()))
    );
    assert!(!form.has_introduced_integers());
    assert!(!form.unsupported.is_empty());
    assert!(!state.integer_introduction_context_live());
    assert!(
        state
            .integer_owner
            .as_ref()
            .unwrap()
            .seen
            .contains("heading")
    );
    assert!(form.submission().is_err());
}
#[test]
fn strict_display_shapes_keep_complete_identity_accounting_and_defensive_utf8_entry_bounds() {
    assert_eq!("\u{e9}".len(), 2);
    let composition = json!({"type":"plain_text","text":""});
    for text in ["".to_owned(), "a".repeat(3000), "\u{e9}".repeat(1500)] {
        let block =
            json!({"type":"section","block_id":"S","text":{"type":"plain_text","text":text}});
        assert!(scan(&view(vec![block])).1.is_some());
    }
    for count in [1, 10] {
        let block =
            json!({"type":"context","block_id":"C","elements":vec![composition.clone();count]});
        assert!(scan(&view(vec![block])).1.is_some());
    }
    let mut rejected = vec![
        json!({"type":"divider"}),
        json!({"type":"divider","block_id":" ","unknown":false}),
        json!({"type":"divider","block_id":"D","unknown":false}),
        json!({"type":"image","block_id":"I","image_url":"https://invalid.example/image"}),
        json!({"type":"actions","block_id":"A","elements":[]}),
        json!({"type":"condition","block_id":"Q","cases":[]}),
        json!({"type":"header","block_id":"H"}),
        json!({"type":"section","block_id":"S","text":{"type":"mrkdwn","text":"text"}}),
        json!({"type":"section","block_id":"S","text":{"type":"plain_text","text":false}}),
        json!({"type":"section","block_id":"S","text":composition,"accessory":{"type":"button"}}),
        json!({"type":"section","block_id":"S","text":composition,"fields":[]}),
        json!({"type":"context","block_id":"C","elements":[]}),
        json!({"type":"context","block_id":"C","elements":vec![composition.clone();11]}),
        json!({"type":"context","block_id":"C","elements":[{"type":"image","image_url":"https://invalid.example/image"}]}),
        json!({"type":"context","block_id":"C","elements":false}),
    ];
    for emoji in [json!(false), Value::Null, json!(1), json!("true")] {
        rejected.push(json!({"type":"section","block_id":"S","text":{"type":"plain_text","text":"text","emoji":emoji}}));
    }
    for text in ["a".repeat(3001), "\u{e9}".repeat(1500) + "a"] {
        rejected.push(
            json!({"type":"section","block_id":"S","text":{"type":"plain_text","text":text}}),
        );
    }
    for block in rejected {
        let (ids, qualified) = scan(&view(vec![
            block.clone(),
            json!({"type":"divider","block_id":"later"}),
        ]));
        assert!(qualified.is_none(), "accepted {block}");
        assert!(
            ids.contains("later"),
            "earlier failure must not stop complete-frame history"
        );
        if let Some(id) = exact_id(block.get("block_id")) {
            assert!(ids.contains(id));
        }
    }
    assert!(
        scan(&view(vec![
            json!({"type":"divider","block_id":"duplicate"}),
            quantity("duplicate")
        ]))
        .1
        .is_none()
    );
}
