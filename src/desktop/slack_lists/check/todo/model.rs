use super::*;
pub(super) fn snapshot(state: Signal<ShellState>) -> Result<Value, String> {
    let s = state.peek();
    if !enabled() || s.core.session.is_some() || s.core.transport.is_some() {
        return Err("Disconnected To-do state required".into());
    }
    let v = s.slack_lists.view.as_ref().ok_or("Missing To-do view")?;
    let l = v.loaded.as_ref().ok_or("Missing To-do records")?;
    let d = crate::slack_lists::record_detail::presentation(v).ok_or("Missing To-do detail")?;
    let ledger = if mode() == "reminder" {
        reminder::fixture::diagnostic(&s)?
    } else {
        completion::fixture::diagnostic(&s)?
    };
    Ok(
        json!({"document":l.document.raw,"population":l.records.raw,"visit":v.generation,"revision":v.revision,
        "selection":v.layout.selection_generation,"find":{"input":v.layout.find.input,"applied":v.layout.find.applied,"revision":v.layout.find.revision},
        "detail":{"record":d.record_id,"generation":d.generation},"ledger":ledger}),
    )
}
pub(super) fn initial(v: &Value) -> Result<(), String> {
    if v["document"]["id"] != "F_LIST_FIXTURE"
        || v["population"]["records"].as_array().map(Vec::len) != Some(73)
        || v["population"]["records"][0]["id"] != "R_LIST_000"
        || v["detail"]["record"] != "R_LIST_000"
        || !v["population"]["response_metadata"]["next_cursor"].is_string()
        || v["ledger"]["forms"] != json!([])
        || v["ledger"]["completions"] != json!([])
        || v["ledger"]["pending"] != false
        || v["population"] != v["ledger"]["backend"]
    {
        return Err("Unexpected initial To-do fixture".into());
    }
    Ok(())
}
pub(super) fn bind_geometry(m: &Value, g: &Value) -> Result<(), String> {
    if g["ok"] != true
        || g["detail"]["record"] != m["detail"]["record"]
        || g["detail"]["generation"].as_str()
            != m["detail"]["generation"]
                .as_u64()
                .map(|n| n.to_string())
                .as_deref()
    {
        return Err("To-do DOM detail differs from model".into());
    }
    if mode() == "reminder" {
        let p = &m["ledger"]["presentation"];
        if p.is_null() != g["modal"].is_null() {
            return Err("Reminder modal/model presence differs".into());
        }
        if p.is_object()
            && (g["modal"]["serial"].as_str()
                != p["serial"].as_u64().map(|n| n.to_string()).as_deref()
                || g["modal"]["input"]["disabled"]
                    != json!(!p["editable"].as_bool().ok_or("Editable shape")?))
        {
            return Err("Reminder DOM serial/editability differs".into());
        }
    } else if m["ledger"]["presentation"].is_null() != g["completion"].is_null() {
        return Err("Completion status differs from reducer".into());
    }
    Ok(())
}
fn reminder_page(base: &Value, due: Option<i64>) -> Value {
    let mut p = base.clone();
    let row = p["records"][0].as_object_mut().unwrap();
    if let Some(due) = due {
        row.insert("saved".into(),json!({"is_archived":false,"state":"in_progress","date_due":due,"date_completed":0,"todo_state":"saved"}));
    } else {
        row.remove("saved");
    }
    p
}
fn completion_page(base: &Value, checked: bool) -> Value {
    let mut p = base.clone();
    let field = p["records"][0]["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["key"] == "todo_completed")
        .unwrap();
    field["checkbox"] = json!(checked);
    p
}
fn reminder_form(method: &str, due: Option<i64>) -> Value {
    let mut f = vec![
        json!(["item_type", "list_record"]),
        json!(["item_id", "F_LIST_FIXTURE"]),
        json!(["ts", "R_LIST_000"]),
    ];
    if let Some(due) = due {
        f.push(json!(["date_due", due.to_string()]));
    }
    f.push(json!([
        "_x_reason",
        match method {
            "saved.add" => "saved-api/saveListRecord",
            "saved.update" => "saved-api/updateSavedListRecord",
            _ => "saved-api/removeSavedListRecord",
        }
    ]));
    json!({"method":method,"fields":f})
}
fn cells(reference: bool, checked: bool) -> Value {
    json!([{"row_id":if reference{"R_SOURCE"}else{"R_LIST_000"},"column_id":"C_TODO_DONE","checkbox":checked}])
}
fn completion_form(reference: bool, checked: bool) -> Value {
    json!({"method":"lists.cells.update","fields":[["list_id",if reference{"F_TODO_SOURCE"}else{"F_LIST_FIXTURE"}],["cells",cells(reference,checked).to_string()],["_x_reason","bulk-update-cells"]]})
}
pub(super) fn validate(
    initial: &Value,
    proof: &[Value],
    final_model: &Value,
    due: &[i64],
) -> Result<(), String> {
    if proof.len() != stages().len() || proof.last().map(|v| &v["model"]) != Some(final_model) {
        return Err("To-do final stage differs".into());
    }
    let page = &initial["population"];
    let reminder = mode() == "reminder";
    if reminder && (due.len() != 2 || due[0] <= 0 || due[1] <= due[0]) {
        return Err("Reminder local time capture invalid".into());
    }
    let expected_forms = if reminder {
        vec![
            reminder_form("saved.add", Some(due[0])),
            reminder_form("saved.update", Some(due[1])),
            reminder_form("saved.delete", None),
        ]
    } else {
        vec![
            completion_form(false, true),
            completion_form(true, true),
            completion_form(false, false),
            completion_form(true, false),
        ]
    };
    let counts: &[usize] = if reminder {
        &[0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3]
    } else {
        &[0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4]
    };
    let completions: &[usize] = if reminder {
        &[0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3]
    } else {
        &[0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4]
    };
    let reads: &[usize] = if reminder {
        &[2, 2, 3, 4, 4, 5, 6, 6, 7, 7, 8, 10, 10, 11]
    } else {
        &[0, 2, 2, 4, 6, 6, 8, 10, 10, 12, 14, 14, 16]
    };
    for (i, stage) in proof.iter().enumerate() {
        let m = &stage["model"];
        let l = &m["ledger"];
        if stage["name"] != stages()[i]
            || m["document"] != initial["document"]
            || m["find"] != initial["find"]
            || m["visit"] != initial["visit"]
            || m["selection"] != initial["selection"]
            || m["detail"]["record"] != initial["detail"]["record"]
            || l["forms"] != json!(&expected_forms[..counts[i]])
            || l["completions"].as_array().map(Vec::len) != Some(completions[i])
            || l["reads"] != json!(reads[i])
            || l["pending"] != json!(counts[i] > completions[i])
        {
            return Err(format!(
                "To-do exact custody/ledger differs at {}",
                stages()[i]
            ));
        }
        bind_geometry(m, &stage["geometry"])?;
        let previous = if i == 0 {
            initial
        } else {
            &proof[i - 1]["model"]
        };
        let refreshed = if reminder {
            [5, 10, 13].contains(&i)
        } else {
            [6, 12].contains(&i)
        };
        for (old, new) in [
            (&previous["revision"], &m["revision"]),
            (
                &previous["detail"]["generation"],
                &m["detail"]["generation"],
            ),
        ] {
            if !old.is_u64() || !new.is_u64() || (old != new) != refreshed {
                return Err("Canonical refresh must rotate revision/detail authority only at observed stages".into());
            }
        }
        if reminder {
            let displayed = if i <= 4 || i == 13 {
                None
            } else if i <= 9 {
                Some(due[0])
            } else {
                Some(due[1])
            };
            let backend = if i <= 3 || i >= 12 {
                None
            } else if i <= 8 {
                Some(due[0])
            } else {
                Some(due[1])
            };
            if m["population"] != reminder_page(page, displayed)
                || l["backend"] != reminder_page(page, backend)
            {
                return Err(format!(
                    "Reminder raw page/cursor changed at {}",
                    stages()[i]
                ));
            }
            for (j, c) in l["completions"].as_array().unwrap().iter().enumerate() {
                if c != &json!({"method":expected_forms[j]["method"],"fields":expected_forms[j]["fields"],"lostReply":j!=1,"applied":true})
                {
                    return Err("Reminder completion evidence differs".into());
                }
            }
        } else {
            let displayed = (6..12).contains(&i);
            let backend = (2..8).contains(&i);
            let mirror = (5..11).contains(&i);
            let reference = &initial["ledger"]["reference"];
            if m["population"] != completion_page(page, displayed)
                || l["backend"] != completion_page(page, backend)
                || l["reference"]["document"] != reference["document"]
                || l["reference"]["records"] != completion_page(&reference["records"], mirror)
            {
                return Err(format!(
                    "Completion raw sibling/reference/cursor changed at {}",
                    stages()[i]
                ));
            }
            for (j, c) in l["completions"].as_array().unwrap().iter().enumerate() {
                if c != &json!({"reference":j%2==1,"cells":cells(j%2==1,j<2),"lostReply":j==0||j==3,"applied":true})
                {
                    return Err("Completion outcome evidence differs".into());
                }
            }
        }
    }
    Ok(())
}
