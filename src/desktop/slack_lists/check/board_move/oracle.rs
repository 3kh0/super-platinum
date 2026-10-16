//! Independent literal population/order and exact bounded native checkpoints.
use super::*;
pub(super) const STAGES: [&str; 8] = [
    "initial",
    "destination",
    "saving-first",
    "uncertain-first",
    "retired-or-missing",
    "observed-first",
    "saving-second",
    "observed-second",
];
pub(super) fn loaded(index: usize) -> usize {
    match index {
        0..=4 => 0,
        5..=6 => 1,
        7 => 2,
        _ => usize::MAX,
    }
}
pub(super) fn groups(checkbox: bool, page: usize, limit: usize) -> Value {
    let mut groups = std::collections::BTreeMap::<String, Vec<usize>>::new();
    for i in 0..73 {
        if [11, 12, 20].contains(&i) || !checkbox && i == 0 && page == 2 {
            continue;
        }
        let key = if checkbox {
            if if i == 0 { page == 1 } else { i % 2 == 1 } {
                "true"
            } else {
                ""
            }
        } else if i == 0 && page == 1 {
            ""
        } else {
            "OPT_PROGRESS"
        };
        groups.entry(key.into()).or_default().push(i);
    }
    json!(groups.into_iter().map(|(key,mut rows)|{rows.sort_by(|a,b|{let n=|i:usize|if i==5{12.5}else{i as f64};n(*a).total_cmp(&n(*b)).then(a.cmp(b))});json!({"key":key,"count":rows.len(),"rows":rows.into_iter().take(limit).map(|i|format!("R_LIST_{i:03}")).collect::<Vec<_>>()})}).collect::<Vec<_>>())
}
fn unordered_groups_equal(actual: &Value, expected: &Value) -> bool {
    let Some(a) = actual.as_array() else {
        return false;
    };
    let Some(e) = expected.as_array() else {
        return false;
    };
    let unique = |groups: &Vec<Value>| {
        groups
            .iter()
            .filter_map(|g| g["key"].as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == groups.len()
    };
    a.len() == e.len()
        && unique(a)
        && unique(e)
        && a.iter().all(|g| e.iter().filter(|w| *w == g).count() == 1)
}
/// Independently recover the original full page by undoing only the exact target cell.
fn exact_pages(pages: &Value, checkbox: bool) -> bool {
    let Some(p) = pages.as_array().filter(|p| p.len() == 3) else {
        return false;
    };
    let Some(rows) = p[0]["records"].as_array().filter(|r| r.len() == 73) else {
        return false;
    };
    let Some(index) = rows.iter().position(|r| r["id"] == "R_LIST_000") else {
        return false;
    };
    if rows.iter().filter(|r| r["id"] == "R_LIST_000").count() != 1 {
        return false;
    }
    for stage in 1..=2 {
        let mut restored = p[stage].clone();
        let Some(fields) = restored["records"][index]["fields"].as_array_mut() else {
            return false;
        };
        let key = if checkbox { "done" } else { "status" };
        let positions = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f["key"] == key)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if positions.len() != 1 {
            return false;
        }
        let at = positions[0];
        if checkbox {
            if fields[at] != json!({"key":"done","checkbox":stage==1}) || at + 1 != fields.len() {
                return false;
            }
            fields.remove(at);
        } else {
            if fields[at]["select"]
                != if stage == 1 {
                    json!([])
                } else {
                    json!(["OPT_DONE"])
                }
            {
                return false;
            }
            fields[at]["select"] = json!(["OPT_PROGRESS"]);
        }
        if restored != p[0] {
            return false;
        }
    }
    true
}
pub(super) fn stable(shell: &ShellState, baseline: &Value, index: usize) -> Result<Value, String> {
    if index >= 8 {
        return Err("Unknown Board Move stage".into());
    }
    let s = adapter::snapshot(shell)?;
    let checkbox = s["checkbox"].as_bool().ok_or("No case")?;
    if !exact_pages(&baseline["pages"], checkbox) {
        return Err("Independent expected page reversal failed".into());
    }
    let writes = [0, 0, 1, 1, 1, 1, 2, 2][index];
    let completed = [0, 0, 0, 1, 1, 1, 1, 2][index];
    let reads = if checkbox {
        [1, 1, 1, 1, 2, 3, 3, 4][index]
    } else {
        [1, 1, 1, 1, 1, 2, 2, 3][index]
    };
    let page = loaded(index);
    let phase = match index {
        2 | 6 => Some("Saving"),
        3 | 4 => Some("Uncertain"),
        _ => None,
    };
    let count = |key: &str| s[key].as_array().map(Vec::len);
    if s["document"] != baseline["document"]
        || s["loadedDocument"] != baseline["document"]
        || s["pages"] != baseline["pages"]
        || s["owner"] != baseline["owner"]
        || s["visit"] != baseline["visit"]
        || s["selection"] != baseline["selection"]
        || s["find"] != baseline["find"]
        || s["expanded"] != baseline["expanded"]
        || s["collapsed"] != baseline["collapsed"]
        || s["loaded"] != baseline["pages"][page]
        || s["server"] != baseline["pages"][completed]
        || s["read"] != 0
        || s["loading"] != false
        || s["missingQueued"] != false
        || s["attemptCount"] != writes
        || s["preparedValid"] != true
        || count("preflights") != Some(writes)
        || count("requests") != Some(writes)
        || count("completed") != Some(completed)
        || count("reads") != Some(reads)
        || s["duplicate"].is_null() != (index < 4)
    {
        return Err(format!(
            "Board Move stage {index} full raw/count/owner proof differs"
        ));
    }
    if phase.is_some_and(|p| s["operation"]["phase"] != p)
        || phase.is_none() && !s["operation"].is_null()
        || s["pending"].is_null() != (!matches!(index, 2 | 6))
        || s["editor"].is_null() != matches!(index, 0 | 5 | 7)
    {
        return Err("Board Move retained phase/editor differs".into());
    }
    for i in 0..writes {
        let a = &s["requests"][i]["attempt"];
        if a != &s["preflights"][i]["attempt"]
            || a["serial"].as_u64().is_none_or(|v| v == 0)
            || a["visit"] != s["visit"]
            || a["selection"] != s["selection"]
            || i == 1 && a["serial"] == s["requests"][0]["attempt"]["serial"]
            || s["requests"][i]["method"] != "POST"
        {
            return Err("Actual mutation identity differs".into());
        }
        if i < completed
            && (s["completed"][i]["attempt"] != *a
                || s["completed"][i]["loaded"] != baseline["pages"][i]
                || s["completed"][i]["operation"]["phase"] != "Uncertain")
        {
            return Err("ACK/load custody differs".into());
        }
    }
    for e in s["reads"].as_array().unwrap() {
        if e["after"]["read"] != 0 || e["kind"] != "local_fixture_read" {
            return Err("Read incomplete or mislabeled".into());
        }
    }
    let p = crate::slack_lists::presentation(shell).ok_or("Missing Board projection")?;
    let actual=json!(p.lanes.iter().map(|l|json!({"key":l.key,"count":l.count,"rows":l.rows.iter().map(|r|r.id.clone()).collect::<Vec<_>>()})).collect::<Vec<_>>());
    if shell.core.session.is_some()
        || shell.core.transport.is_some()
        || p.layout_kind != crate::slack_lists::ProjectionLayout::Board
        || p.selected_view.as_deref() != Some("V_MOVE")
        || p.view_notice.is_some()
        || !p.incomplete
        || !unordered_groups_equal(&actual, &groups(checkbox, page, usize::MAX))
    {
        return Err("Actual Board lane/order projection differs".into());
    }
    Ok(s)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_order_excludes_children_missing_name_and_filtered_destination() {
        let before = groups(false, 0, usize::MAX);
        assert_eq!(before[0]["count"], 70);
        let moved = groups(false, 1, usize::MAX);
        assert_eq!(moved[0]["key"], "");
        assert_eq!(moved[0]["rows"], json!(["R_LIST_000"]));
        let gone = groups(false, 2, usize::MAX);
        assert_eq!(gone[0]["count"], 69);
        assert!(!gone.to_string().contains("R_LIST_000"));
        for forbidden in ["R_LIST_011", "R_LIST_012", "R_LIST_020"] {
            assert!(!before.to_string().contains(forbidden));
        }
    }
    #[test]
    fn exact_groups_refuse_missing_duplicate_or_reordered_cards() {
        let want = groups(true, 1, 10);
        assert!(unordered_groups_equal(&want, &want));
        let mut wrong = want.clone();
        wrong[0]["rows"].as_array_mut().unwrap().swap(0, 1);
        assert!(!unordered_groups_equal(&wrong, &want));
        let duplicate = json!([want[0], want[0]]);
        assert!(!unordered_groups_equal(&duplicate, &want));
        assert!(!unordered_groups_equal(&want, &duplicate));
        assert!(!unordered_groups_equal(&json!([want[0]]), &want));
    }
}
