use super::*;

#[test]
fn replacement_waits_for_its_own_held_family_and_wire_query() {
    let old = wire::Query::new("oldDana").unwrap();
    let next = wire::Query::new("#research").unwrap();
    let mut ledger = json!({"held":[2],"requests":[
        {"serial":1,"method":"api.getFlannelHttpUrl","query":""},
        {"serial":2,"method":"users/search","query":"oldDana"}
    ]});
    assert_eq!(held_query(&ledger, &old), Some(2));
    assert_eq!(held_query(&ledger, &next), None);
    ledger["requests"].as_array_mut().unwrap().extend([
        json!({"serial":3,"method":"api.getFlannelHttpUrl","query":""}),
        json!({"serial":4,"method":"channels/search","query":"research"}),
    ]);
    assert_eq!(held_query(&ledger, &next), None);
    ledger["held"] = json!([4, 2]);
    assert_eq!(held_query(&ledger, &next), Some(4));
    assert_eq!(held_query(&ledger, &old), Some(2));
    ledger["requests"][3]["method"] = json!("users/search");
    assert_eq!(held_query(&ledger, &next), None);
}

#[test]
fn prefixed_member_query_uses_core_wire_and_refuses_ambiguous_sender() {
    let query = wire::Query::new("@Dana").unwrap();
    let mut ledger = json!({"held":[2],"requests":[
        {"serial":2,"method":"users/search","query":"Dana"}
    ]});
    assert_eq!(held_query(&ledger, &query), Some(2));
    ledger["requests"][0]["query"] = json!("@Dana");
    assert_eq!(held_query(&ledger, &query), None);
    ledger["requests"][0]["query"] = json!("Dana");
    ledger["requests"]
        .as_array_mut()
        .unwrap()
        .push(json!({"serial":3,"method":"users/search","query":"Dana"}));
    ledger["held"] = json!([2, 3]);
    assert_eq!(held_query(&ledger, &query), None);
}
