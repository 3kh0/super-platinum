//! Independent family membership replies; no state inferred from ACKs.
use super::*;
pub(crate) const FIRST: &str = "1781524801.000001";
pub(crate) const SELECTED: &str = "thumbsup::skin-tone-6";
pub(crate) const OWN: &[&str] = &[
    "thumbsup",
    "thumbsup::skin-tone-3",
    "thumbsup::skin-tone-2",
    "thumbsup::skin-tone-4",
];
pub(crate) fn enabled(s: &ShellState) -> bool {
    compose::enabled(s)
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("reaction-family" | "reaction-family-failures")
        )
}
pub(crate) fn reactions(removed: &[&str]) -> Value {
    let mut rows = vec![];
    for name in OWN {
        let mut users = if *name == "thumbsup" {
            vec!["U0", "U1"]
        } else {
            vec!["U0"]
        };
        if removed.contains(name) {
            users.retain(|u| *u != "U0");
        }
        if !users.is_empty() {
            rows.push(json!({"name":name,"count":users.len(),"users":users}));
        }
    }
    for (name, user) in [(SELECTED, "U2"), ("+1::skin-tone-3", "U0"), ("wave", "U1")] {
        rows.push(json!({"name":name,"count":1,"users":[user]}));
    }
    json!(rows)
}
pub(super) fn decorate(raw: &mut Value) {
    if let Some(row) = raw["messages"]
        .as_array_mut()
        .and_then(|r| r.iter_mut().find(|r| r["ts"] == FIRST))
    {
        row["reactions"] = reactions(&[]);
        row["family_fixture_opaque"] = json!({"keep":"raw bases are distinct"});
    }
}
pub(crate) fn server(s: &ShellState, model: &Model, removed: &[&str]) -> Result<Value, String> {
    if !enabled(s)
        || !s
            .slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
        || removed.iter().any(|n| !OWN.contains(n))
    {
        return Err("Original family source required".into());
    }
    let mut l = model.0.lock().map_err(|_| "Family ledger poisoned")?;
    l.reaction_readbacks
        .insert(FIRST.into(), reactions(removed));
    let mut raw = reply(ROOT, false);
    decorate(&mut raw);
    apply_reaction_readbacks(&l.reaction_readbacks, &mut raw);
    Ok(raw)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_server_keeps_alias_and_others_without_mutating_original() {
        let mut raw = reply(ROOT, false);
        decorate(&mut raw);
        let original = raw.clone();
        let mut future = raw.clone();
        apply_reaction_readbacks(
            &BTreeMap::from([(FIRST.into(), reactions(OWN))]),
            &mut future,
        );
        assert_eq!(raw, original);
        assert_eq!(
            future["messages"][1]["reactions"],
            json!([
            {"name":"thumbsup","count":1,"users":["U1"]},{"name":SELECTED,"count":1,"users":["U2"]},{"name":"+1::skin-tone-3","count":1,"users":["U0"]},{"name":"wave","count":1,"users":["U1"]}])
        );
    }
}
