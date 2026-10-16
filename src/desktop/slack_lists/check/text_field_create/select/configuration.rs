//! Preserve the pre-fill event evidence while binding the admitted choice epoch.
use super::*;

pub(in super::super) fn admission(
    initial: &Value,
    configured: &Value,
    current: Value,
    mode: FieldCreateMode,
) -> Result<Value, String> {
    let m = &configured["state"]["model"];
    if !mode.is_select()
        || m["format"] != mode.select_format()
        || configured["options"]["format"] != m["format"]
        || !m["choice"].as_u64().is_some_and(|v| v > 0)
        || (m["choice"] != initial["choice"]) != (mode == FieldCreateMode::MultiSelect)
        || ["serial", "visit", "selection"]
            .iter()
            .any(|k| m[k] != initial[k])
    {
        return Err("Post-fill Select configuration owner/choice mismatch".into());
    }
    let mut expected = initial.clone();
    expected["choice"] = m["choice"].clone();
    if current != expected {
        return Err("Post-fill Select model changed beyond captured choice".into());
    }
    Ok(current)
}

pub(in super::super) fn retained(
    admitted: &Value,
    actual: &Value,
    visit: u64,
    selection: u64,
) -> bool {
    let mut expected = admitted.clone();
    expected["visit"] = json!(visit);
    expected["selection"] = json!(selection);
    *actual == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multi_post_format_choice_is_the_retirement_baseline_without_rewriting_create() {
        let initial = json!({"serial":8,"visit":2,"selection":3,"name":"Release phase","view":"V_FILTER","observed":null,"kind":"select","choice":41});
        let original_evidence = initial.clone();
        for mode in [FieldCreateMode::Select, FieldCreateMode::MultiSelect] {
            let next = if mode == FieldCreateMode::MultiSelect {
                42
            } else {
                41
            };
            let configured = json!({"state":{"model":{"serial":8,"visit":2,"selection":3,"choice":next,"format":mode.select_format()}},"options":{"format":mode.select_format()}});
            let mut post = initial.clone();
            post["choice"] = json!(next);
            let admitted = admission(&initial, &configured, post.clone(), mode).unwrap();
            assert_eq!(initial, original_evidence);
            let mut actual = post.clone();
            actual["visit"] = json!(7);
            actual["selection"] = json!(9);
            assert!(retained(&admitted, &actual, 7, 9));
            assert_eq!(
                retained(&initial, &actual, 7, 9),
                mode == FieldCreateMode::Select
            );
            for key in ["serial", "choice", "name", "view", "kind", "observed"] {
                let mut bad = actual.clone();
                bad[key] = json!("changed");
                assert!(!retained(&admitted, &bad, 7, 9));
            }
            for key in [
                "serial",
                "visit",
                "selection",
                "name",
                "view",
                "kind",
                "observed",
                "choice",
            ] {
                let mut bad = post.clone();
                bad[key] = json!("changed");
                assert!(admission(&initial, &configured, bad, mode).is_err());
            }
            let mut bad = configured.clone();
            bad["state"]["model"]["choice"] = json!(41);
            if mode == FieldCreateMode::MultiSelect {
                assert!(admission(&initial, &bad, post.clone(), mode).is_err());
            }
            bad["state"]["model"]["format"] = json!("unknown");
            assert!(admission(&initial, &bad, post, mode).is_err());
        }
    }
}
