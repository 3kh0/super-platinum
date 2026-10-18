//! Literal full-refinement wire contract, independent of permit expected metadata.
use super::*;
pub(super) fn sorts(index: usize, ordered: bool) -> Option<Value> {
    match (ordered, index) {
        (false, 0) => Some(json!([{"key":"estimate","ascending":true}])),
        (false, 1) | (true, 2) => Some(json!([{"key":"estimate","ascending":false}])),
        (false, 2) | (true, 3) => Some(json!([])),
        (true, 0) => {
            Some(json!([{"key":"done","ascending":true},{"key":"estimate","ascending":false}]))
        }
        (true, 1) => {
            Some(json!([{"key":"estimate","ascending":false},{"key":"done","ascending":true}]))
        }
        _ => None,
    }
}
fn form(values: Vec<(&str, String)>) -> Vec<(String, String)> {
    std::iter::once(("token".into(), "fixture-not-a-token".into()))
        .chain(values.into_iter().map(|(k, v)| (k.into(), v)))
        .collect()
}
fn exact(r: &PreparedRequest, path: &str, fields: Vec<(String, String)>) -> Result<(), Error> {
    if r.method != "POST"
        || url::Url::parse(&r.url)
            .ok()
            .is_none_or(|u| u.host_str() != Some("example.test") || u.path() != path)
        || !matches!(&r.body,RequestBody::Form(f) if f==&fields)
    {
        return Err(Error::Api("sort_refinement_exact_form".into()));
    }
    Ok(())
}
pub(super) fn metadata(r: &PreparedRequest, sorts: &Value) -> Result<(), Error> {
    exact(
        r,
        "/api/lists.views.update",
        form(vec![
            ("list_id", "F_LIST_FIXTURE".into()),
            ("id", "V_SORT".into()),
            ("type", "table".into()),
            ("filters", "[]".into()),
            ("info_column_filters", "[]".into()),
            ("sorts", sorts.to_string()),
            ("grouping", "{}".into()),
            ("show_completed_items", "false".into()),
            ("_x_reason", "update-view".into()),
        ]),
    )
}
pub(super) fn saved(original: &Value, sorts: &Value) -> Result<Value, Error> {
    let mut raw = original.clone();
    let view = raw["list_metadata"]["views"]
        .as_array_mut()
        .and_then(|v| v.iter_mut().find(|v| v["id"] == "V_SORT"))
        .ok_or_else(|| Error::Api("sort_refinement_view".into()))?;
    view["type"] = json!("table");
    view["filters"] = json!([]);
    view["info_column_filters"] = json!([]);
    view["sorts"] = sorts.clone();
    view["grouping"] = json!({});
    view["show_completed_items"] = json!(false);
    Ok(raw)
}
pub(super) fn preferences(saved: bool) -> Value {
    let encoded = if saved {
        json!({"F_LIST_FIXTURE":"V_SORT","F_OTHER":"V_OTHER","opaque":"retained"}).to_string()
    } else {
        "{ \"F_OTHER\": \"V_OTHER\", \"F_LIST_FIXTURE\": \"V_PRECISION\", \"opaque\": \"retained\" }".into()
    };
    json!({"ok":true,"prefs":{"lists_default_view":encoded}})
}
pub(super) fn preference_read(r: &PreparedRequest) -> Result<(), Error> {
    exact(
        r,
        "/api/users.prefs.get",
        form(vec![
            ("prefs", "lists_default_view".into()),
            ("_x_reason", "native-list-default-view-read".into()),
        ]),
    )
}
pub(super) fn preference_write(r: &PreparedRequest) -> Result<(), Error> {
    exact(
        r,
        "/api/users.prefs.set",
        form(vec![
            ("name", "lists_default_view".into()),
            (
                "value",
                preferences(true)["prefs"]["lists_default_view"]
                    .as_str()
                    .unwrap()
                    .into(),
            ),
            ("_x_reason", "prefs-api/setUserPrefByApi".into()),
        ]),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_refinement_result_preserves_unknown_metadata_rows_and_sibling_views() {
        let raw = json!({"id":"F_LIST_FIXTURE","opaque":[null,2,true],"list_metadata":{"schema":[{"unknown":3}],"views":[{"id":"V_SORT","name":"Shared estimates","sorts":[],"columns":[{"opaque":9}],"unknown":{"keep":true}},{"id":"other","sorts":[{"unknown":1}]}]}});
        let next = saved(&raw, &sorts(0, true).unwrap()).unwrap();
        assert_eq!(
            next["list_metadata"]["views"][1],
            raw["list_metadata"]["views"][1]
        );
        assert_eq!(
            next["list_metadata"]["schema"],
            raw["list_metadata"]["schema"]
        );
        assert_eq!(next["opaque"], raw["opaque"]);
        let mut undo = next;
        let v = &mut undo["list_metadata"]["views"][0];
        for key in [
            "type",
            "filters",
            "info_column_filters",
            "grouping",
            "show_completed_items",
        ] {
            v.as_object_mut().unwrap().remove(key);
        }
        v["sorts"] = json!([]);
        assert_eq!(undo, raw);
    }
    #[test]
    fn exact_priority_remove_clear_and_preference_map_are_closed() {
        assert_ne!(sorts(0, true), sorts(1, true));
        assert_eq!(sorts(2, true), sorts(1, false));
        assert_eq!(sorts(3, true), Some(json!([])));
        assert_eq!(sorts(2, false), Some(json!([])));
        assert!(sorts(4, true).is_none() && sorts(3, false).is_none());
        let before: Value = serde_json::from_str(
            preferences(false)["prefs"]["lists_default_view"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let mut after: Value = serde_json::from_str(
            preferences(true)["prefs"]["lists_default_view"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        after["F_LIST_FIXTURE"] = before["F_LIST_FIXTURE"].clone();
        assert_eq!(before, after);
    }
}
