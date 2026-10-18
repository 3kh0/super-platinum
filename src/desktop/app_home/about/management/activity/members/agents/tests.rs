use super::*;
use serde_json::json;

#[test]
fn observed_agent_profile_does_not_admit_self_plain_bot_or_changed_member() {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let agent = json!({"id":"U12345678","is_bot":true,"profile":{"api_app_id":"AAGENT"}});
    let plain = json!({"id":"U87654321","is_bot":true,"profile":{"api_app_id":"APLAIN"}});
    let myself = json!({"id":q.owner.ctx.session.user_id});
    let rows = vec![agent.clone(), plain.clone(), myself.clone()];
    let mut value = Acquired {
        generation: q.generation,
        indices: rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r["id"].as_str().unwrap().to_owned(), i))
            .collect(),
        rows,
        profiles: BTreeMap::new(),
        access: None,
        receipts: vec![],
    };
    assert!(!value.allowed(&q, &agent));
    let raw = json!({"ok":true,"app_profile":{"id":"AAGENT","is_agent_app":true}});
    value.profiles.insert(
        "AAGENT".into(),
        facts::Profile::decode("AAGENT", &raw).unwrap(),
    );
    value.receipts.push(raw);
    assert!(value.allowed(&q, &agent));
    assert!(!value.allowed(&q, &plain));
    assert!(!value.allowed(&q, &myself));
    let mut changed = agent.clone();
    changed["deleted"] = json!(true);
    assert!(!value.allowed(&q, &changed));
    let next = Query::capture(&mut s, visit).unwrap();
    assert!(!value.allowed(&next, &agent));
    assert!(!value.current(&s, &q, &value.rows));
}

#[test]
fn client_agentforce_requires_actual_access_beyond_matching_agent_profile() {
    let (mut s, visit) = super::super::tests::setup();
    let q = Query::capture(&mut s, visit).unwrap();
    let raw = json!({"id":"U12345678","is_bot":true,"is_agentforce_bot":true,"profile":{"api_app_id":"AAGENT"}});
    let profile=facts::Profile::decode("AAGENT",&json!({"ok":true,"app_profile":{"id":"AAGENT","is_agent_app":true,"agentforce":{},"bot_user":{"id":"U12345678"}}})).unwrap();
    let profiles = BTreeMap::from([("AAGENT".into(), profile)]);
    let mut value = Acquired {
        generation: q.generation,
        rows: vec![raw.clone()],
        indices: BTreeMap::from([("U12345678".into(), 0)]),
        profiles: profiles.clone(),
        access: None,
        receipts: vec![],
    };
    assert!(!value.allowed(&q, &raw));
    value.access = Some(facts::Access::new(
        facts::Settings::decode(&json!({}), None).unwrap(),
        BTreeSet::from(["AAGENT".into()]),
        profiles,
    ));
    assert!(value.allowed(&q, &raw));
}
