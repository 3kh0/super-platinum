//! Literal named scenario forms, independent of the intent's expected document.
use super::*;
fn form(fields: Vec<(&str, String)>) -> Vec<(String, String)> {
    std::iter::once(("token".into(), "fixture-not-a-token".into()))
        .chain(fields.into_iter().map(|(k, v)| (k.into(), v)))
        .collect()
}
fn exact(
    request: &PreparedRequest,
    path: &str,
    expected: &[(String, String)],
) -> Result<(), Error> {
    if request.method != "POST"
        || url::Url::parse(&request.url)
            .ok()
            .is_none_or(|u| u.path() != path)
        || !matches!(&request.body,RequestBody::Form(f) if f==expected)
    {
        return Err(Error::Api("fixture_refinement_exact_form".into()));
    }
    Ok(())
}
fn grouping() -> Value {
    json!({"group_by":"status","opaque":{"retained":[1,null,true]},"order":[{"select":[]},{"select":["OPT_DONE"]},{"select":["OPT_PROGRESS"]}]})
}
fn columns(created: bool) -> Value {
    if created {
        json!([{"id":"C_NAME","visible":true},{"id":"C_NUMBER","visible":false}])
    } else {
        json!([{"id":"C_NUMBER","key":"estimate","visible":false}])
    }
}
fn fields(mode: Case) -> Vec<(String, String)> {
    let created = mode.created();
    let mut f = vec![("list_id", "F_LIST_FIXTURE".into())];
    if created {
        f.push(("name", "Focused delivery".into()));
    } else {
        f.push(("id", "V_LANE".into()));
    }
    f.push(("type", "board".into()));
    if created {
        f.push(("grouping", grouping().to_string()));
    } else {
        f.extend([
            ("filters", "[]".into()),
            ("info_column_filters", "[]".into()),
            ("sorts", "[]".into()),
        ]);
    }
    f.push(("columns", columns(created).to_string()));
    if !created {
        f.push(("grouping", grouping().to_string()));
    }
    f.extend([
        ("show_completed_items", "false".into()),
        (
            "_x_reason",
            if created {
                "create-view"
            } else {
                "update-view"
            }
            .into(),
        ),
    ]);
    form(f)
}
pub(super) fn metadata(request: &PreparedRequest, mode: Case) -> Result<(), Error> {
    exact(
        request,
        if mode.created() {
            "/api/lists.views.create"
        } else {
            "/api/lists.views.update"
        },
        &fields(mode),
    )
}
pub(super) fn saved(original: &Value, mode: Case) -> Result<Value, Error> {
    let mut out = original.clone();
    let views = out["list_metadata"]["views"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_views".into()))?;
    let view = if mode.created() {
        if views.iter().any(|v| v["id"] == mode.view_id()) {
            return Err(Error::Api("fixture_duplicate_created_view".into()));
        }
        views.push(json!({"id":mode.view_id(),"name":"Focused delivery","type":"board","position":"0000000009","columns":[{"id":"C_NAME","key":"NAME","visible":true},{"id":"C_NUMBER","key":"estimate","visible":false}]}));
        views.last_mut().unwrap()
    } else {
        let view = views
            .iter_mut()
            .find(|v| v["id"] == "V_LANE")
            .ok_or_else(|| Error::Api("fixture_view".into()))?;
        let column = view["columns"]
            .as_array_mut()
            .and_then(|a| a.iter_mut().find(|c| c["id"] == "C_NUMBER"))
            .ok_or_else(|| Error::Api("fixture_column".into()))?;
        column["visible"] = json!(false);
        view["filters"] = json!([]);
        view["info_column_filters"] = json!([]);
        view["sorts"] = json!([]);
        view
    };
    view["grouping"] = grouping();
    view["show_completed_items"] = json!(false);
    Ok(out)
}
pub(super) fn original_preferences() -> Value {
    json!({"ok":true,"prefs":{"lists_default_view":"{ \"F_OTHER\": \"V_OTHER\", \"F_LIST_FIXTURE\": \"V_PRECISION\", \"opaque\": \"retained\" }"}})
}
pub(super) fn preferences(mode: Case) -> Value {
    json!({"F_OTHER":"V_OTHER","F_LIST_FIXTURE":mode.view_id(),"opaque":"retained"})
}
pub(super) fn preference_read(request: &PreparedRequest) -> Result<(), Error> {
    exact(
        request,
        "/api/users.prefs.get",
        &form(vec![
            ("prefs", "lists_default_view".into()),
            ("_x_reason", "native-list-default-view-read".into()),
        ]),
    )
}
pub(super) fn preference_write(request: &PreparedRequest, mode: Case) -> Result<(), Error> {
    exact(
        request,
        "/api/users.prefs.set",
        &form(vec![
            ("name", "lists_default_view".into()),
            ("value", preferences(mode).to_string()),
            ("_x_reason", "prefs-api/setUserPrefByApi".into()),
        ]),
    )
}
