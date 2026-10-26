use super::*;
pub(super) fn op(block: &str, value: &str, commit: &str) -> Value {
    json!({"id":crate::view::app_modal::field_id(&key(block)),"block":block,"child":"time","value":value,"commit":commit,"set":true})
}
pub(super) fn commit(block: &str, mode: &str) -> Value {
    json!({"id":crate::view::app_modal::field_id(&key(block)),"block":block,"child":"time","value":"","commit":mode,"set":false})
}
pub(super) async fn edit(
    s: Signal<ShellState>,
    ops: Vec<Value>,
    notes: Option<&str>,
) -> Result<Value, String> {
    let started = Instant::now();
    let before = snapshot(&s.peek());
    let mut proof = capture::dom(&format!(
        "await (async()=>{{{} const ops={},notes={}; {} }})()",
        capture::ACTION_GUARD,
        json!(ops),
        json!(
            notes.map(|v| json!({"id":crate::view::app_modal::field_id(&key("notes")),"value":v,"visit":before["authority"]["homeVisit"],"revision":before["authority"]["homeRevision"]}))
        ),
        include_str!("gesture.js")
    ))
    .await?;
    if let Some(wanted) = notes {
        let mut acknowledged = false;
        for _ in 0..150 {
            let x = snapshot(&s.peek());
            if x["authority"] != before["authority"] || x["rawView"] != before["rawView"] {
                return Err("Quiet sibling source retired during acknowledgment".into());
            }
            if x["fields"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|f| f["block"] == "notes" && f["value"] == wanted)
            }) {
                acknowledged = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if !acknowledged {
            return Err("Quiet sibling input was not acknowledged by current form".into());
        }
        proof["sibling"]["modelAcknowledged"] = json!(true);
    }
    Ok(
        json!({"dom":proof,"before":before,"after":snapshot(&s.peek()),"modelAckElapsedMs":started.elapsed().as_millis()}),
    )
}

pub(super) fn choose(block: &str, value: &str) -> Value {
    json!({"id":crate::view::app_modal::field_id(&key(block)),"block":block,"child":"time","value":value,"commit":"option","set":false})
}

pub(super) fn date(block: &str, value: &str, mode: &str) -> Value {
    // Preserve the old selection callback via the actual editable text Enter path.
    // A chooser selection moves focus and would commit the counterpart Time draft.
    let commit = if mode == "change" { "enter" } else { mode };
    let mut x = op(block, value, commit);
    x["child"] = json!("date");
    x["intent"] = json!(if mode == "change" {
        "date-text-enter-compatibility"
    } else {
        "date-text-observation"
    });
    x
}
pub(super) fn date_without_focus(block: &str, value: &str) -> Value {
    let mut x = date(block, value, "change");
    x["focus"] = json!(false);
    x
}
