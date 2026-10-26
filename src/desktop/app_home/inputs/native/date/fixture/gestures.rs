use super::*;
pub(crate) fn op(block: &str, value: &str, commit: &str) -> Value {
    json!({"id":crate::view::app_modal::field_id(&key(block)),"block":block,"value":value,"commit":commit,"set":true})
}
pub(crate) fn commit(block: &str, mode: &str) -> Value {
    json!({"id":crate::view::app_modal::field_id(&key(block)),"block":block,"value":"","commit":mode,"set":false})
}
pub(crate) async fn edit(
    s: Signal<ShellState>,
    mut ops: Vec<Value>,
    notes: Option<&str>,
) -> Result<Value, String> {
    let started = Instant::now();
    let before = snapshot(&s.peek());
    for op in &mut ops {
        if op["commit"] == "change" && op["set"] == false {
            let field = before["dateWidgets"]
                .as_array()
                .and_then(|a| a.iter().find(|w| w["block"] == op["block"]))
                .ok_or("Date picker commit lost owned parsed observation")?;
            op["pickerValue"] = field["parsed"]
                .as_str()
                .map(|v| json!(v))
                .or_else(|| (field["raw"] == "").then(|| json!("")))
                .ok_or("Date picker commit requires acknowledged canonical draft")?;
        }
    }
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
