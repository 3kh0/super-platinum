//! Exact owned Select draft proof, bounded to this disconnected create diagnostic.
use super::*;
use super_platinum_core::slack_lists::SelectCreateOptions;
pub(crate) const LABELS: [&str; 8] = [
    "Ready",
    "Équipe 東京",
    "Review",
    "Approved",
    "In progress",
    "Waiting",
    "Complete",
    "Final option",
];
pub(crate) fn validate(options: &Value) -> Result<(), String> {
    validate_mode(FieldCreateMode::Select, options)
}
pub(crate) fn validate_mode(mode: FieldCreateMode, options: &Value) -> Result<(), String> {
    let typed = SelectCreateOptions::from_wire(options).map_err(|e| e.to_string())?;
    if !mode.is_select()
        || typed.format().as_str() != mode.select_format()
        || typed
            .choices()
            .iter()
            .map(|c| c.label())
            .collect::<Vec<_>>()
            != LABELS
    {
        return Err("Unexpected Select fixture labels/order".into());
    }
    Ok(())
}
pub(crate) fn draft(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) || !field_create_mode().is_some_and(FieldCreateMode::is_select) {
        return Err("Exact Select fixture required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.target == Target::NewTextField("V_FILTER".into())
                && e.field_choice.kind == ColumnCreateKind::Select
        })
        .ok_or("Missing owned Select draft")?;
    let d = &e.select_draft;
    Ok(
        json!({"serial":e.serial,"visit":generation(shell),"selection":selection(shell),"choice":e.field_choice.revision,"epoch":d.add_epoch,"format":d.format.as_str(),"pending":d.pending,"ready":d.ready(),"rows":d.rows.iter().map(|r|json!({"generation":r.generation,"label":r.option.label(),"value":r.option.value(),"color":r.option.color()})).collect::<Vec<_>>()}),
    )
}
pub(crate) fn bind(shell: &ShellState, serial: u64, options: Value) -> Result<(), String> {
    validate_mode(field_create_mode().ok_or("Missing mode")?, &options)?;
    if !enabled(shell) || !field_create_mode().is_some_and(FieldCreateMode::is_select) {
        return Err("Exact Select mode required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.serial == serial
                && e.stage == Stage::Editing
                && e.job.is_none()
                && e.intent.is_none()
                && e.field_choice.kind == ColumnCreateKind::Select
                && e.target == Target::NewTextField("V_FILTER".into())
        })
        .ok_or("Select bind lacks exact editable owner")?;
    if e.select_draft.capture().map_err(|e| e.to_string())?.wire() != options {
        return Err("Independent DOM options differ from typed draft".into());
    }
    let mut l = ledger().lock().unwrap();
    if l.select_options.is_some()
        || !l.requests.is_empty()
        || !l.finished.is_empty()
        || l.pending.is_some()
        || l.held_read.is_some()
    {
        return Err("Select proof already spent or requests started".into());
    }
    l.select_options = Some((serial, options));
    Ok(())
}
pub(crate) fn bound_options(shell: &ShellState, serial: u64) -> Result<Value, String> {
    if !enabled(shell) || !field_create_mode().is_some_and(FieldCreateMode::is_select) {
        return Err("Exact Select mode required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.serial == serial
                && e.field_choice.kind == ColumnCreateKind::Select
                && e.target == Target::NewTextField("V_FILTER".into())
        })
        .ok_or("Select proof owner absent")?;
    let Some(Intent::NewTextField(intent)) = &e.intent else {
        return Err("Missing captured Select intent".into());
    };
    let l = ledger().lock().unwrap();
    let (id, options) = l
        .select_options
        .as_ref()
        .ok_or("Independent options not bound")?;
    if *id != serial || intent.options().as_ref() != Some(options) {
        return Err("Select captured options drift".into());
    }
    validate_mode(field_create_mode().ok_or("Missing mode")?, options)?;
    Ok(options.clone())
}
pub(super) fn add_field(server: &mut Value, options: &Value) -> Result<(), Error> {
    add_field_mode(FieldCreateMode::Select, server, options)
}
pub(super) fn add_field_mode(
    mode: FieldCreateMode,
    server: &mut Value,
    options: &Value,
) -> Result<(), Error> {
    validate_mode(mode, options).map_err(|_| Error::Api("fixture_select_options".into()))?;
    let schema = server["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_schema".into()))?;
    if schema
        .iter()
        .any(|c| c["id"] == "C_SELECT_OBSERVED" || c["key"] == "select_observed")
    {
        return Err(Error::Api("fixture_duplicate".into()));
    }
    schema.insert(1,json!({"id":"C_SELECT_OBSERVED","key":"select_observed","name":"Release phase","type":"select","options":options,"opaque":{"fixture":"retained"}}));
    let columns = server["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_columns".into()))?;
    columns.insert(
        1,
        json!({"id":"C_SELECT_OBSERVED","key":"select_observed","visible":true}),
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_five_way_gate_and_independent_option_config() {
        for alias in [
            "slack-list-select-field-create",
            "slack-list-select-field-create-narrow",
        ] {
            assert_eq!(
                mode_gate_all(None, None, None, None, Some("1"), Some(alias)),
                Some(FieldCreateMode::Select)
            );
            for wrong in [Some(""), Some("0"), Some("1")] {
                assert!(mode_gate_all(wrong, None, None, None, Some("1"), Some(alias)).is_none());
                assert!(mode_gate_all(None, wrong, None, None, Some("1"), Some(alias)).is_none());
                assert!(mode_gate_all(None, None, wrong, None, Some("1"), Some(alias)).is_none());
                assert!(mode_gate_all(None, None, None, wrong, Some("1"), Some(alias)).is_none());
            }
            for flag in [None, Some(""), Some("0")] {
                assert!(mode_gate_all(None, None, None, None, flag, Some(alias)).is_none());
            }
        }
        assert!(
            mode_gate_all(
                None,
                None,
                None,
                None,
                Some("1"),
                Some("slack-list-date-field-create")
            )
            .is_none()
        );
        let options = json!({"format":"single_select","choices":LABELS.iter().enumerate().map(|(i,label)|json!({"label":label,"value":format!("OptAAAAAAA{i}"),"color":"blue"})).collect::<Vec<_>>()});
        validate(&options).unwrap();
        for which in 0..6 {
            let mut bad = options.clone();
            match which {
                0 => bad["choices"][0]["value"] = json!("wrong"),
                1 => bad["choices"][0]["color"] = json!("red"),
                2 => bad["choices"][0]["label"] = json!("different"),
                3 => bad["choices"].as_array_mut().unwrap().swap(0, 1),
                4 => {
                    bad["choices"].as_array_mut().unwrap().pop();
                }
                _ => bad["default_value"] = json!("OptAAAAAAA0"),
            };
            assert!(validate(&bad).is_err());
        }
        let mut doc =
            json!({"list_metadata":{"schema":[{"id":"old"}],"views":[{"columns":[{"id":"old"}]}]}});
        let before = doc.clone();
        add_field(&mut doc, &options).unwrap();
        assert_eq!(doc["list_metadata"]["schema"][1]["options"], options);
        assert!(add_field(&mut doc, &options).is_err());
        doc["list_metadata"]["schema"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        doc["list_metadata"]["views"][0]["columns"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        assert_eq!(doc, before);
    }
}

#[cfg(test)]
mod multi_gate_tests {
    use super::*;
    #[test]
    fn closed_multi_mode_cannot_broaden_single_or_admit_mixed_flags() {
        for alias in [
            "slack-list-multi-select-field-create",
            "slack-list-multi-select-field-create-narrow",
        ] {
            assert_eq!(
                mode_gate_multi(None, None, None, None, None, Some("1"), Some(alias)),
                Some(FieldCreateMode::MultiSelect)
            );
            for flag in [Some(""), Some("0"), Some("1")] {
                for index in 0..5 {
                    let mut flags = [None; 5];
                    flags[index] = flag;
                    assert!(
                        mode_gate_multi(
                            flags[0],
                            flags[1],
                            flags[2],
                            flags[3],
                            flags[4],
                            Some("1"),
                            Some(alias)
                        )
                        .is_none()
                    );
                }
            }
        }
        assert!(
            mode_gate_multi(
                None,
                None,
                None,
                None,
                None,
                Some("1"),
                Some("slack-list-select-field-create")
            )
            .is_none()
        );
        let mut options = json!({"format":"multi_select","choices":LABELS.iter().enumerate().map(|(i,l)|json!({"label":l,"value":format!("OptAAAAAAA{i}"),"color":"blue"})).collect::<Vec<_>>()});
        validate_mode(FieldCreateMode::MultiSelect, &options).unwrap();
        assert!(validate(&options).is_err());
        options["format"] = json!("single_select");
        validate(&options).unwrap();
        assert!(validate_mode(FieldCreateMode::MultiSelect, &options).is_err());
        for mode in [
            FieldCreateMode::Text,
            FieldCreateMode::Number,
            FieldCreateMode::Checkbox,
            FieldCreateMode::Date,
        ] {
            assert!(validate_mode(mode, &options).is_err());
        }
    }
}
