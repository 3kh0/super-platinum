//! Passive named failure diagnostics; never grants selection/write authority.
use super::*;
pub(super) async fn capture(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
) -> Value {
    let (runtime, snapshot) = {
        let shell = state.peek();
        match crate::canvas::checklist::presentation(&shell) {
            Some(v) => (
                json!({"visit":v.visit,"read":v.read,"loading":v.loading,"stale":v.stale,"operation":v.operation.map(|(serial,section,stage)|json!({"serial":serial,"section":section,"stage":format!("{stage:?}")})),"capacity":v.capacity_reason,"error":v.error}),
                v.snapshot.clone(),
            ),
            None => (Value::Null, None),
        }
    };
    let original=tokio::task::spawn_blocking(move||{
  snapshot.map(|s|json!(data::IDS.iter().map(|id|{
   let result=super::super::range::capture_intent(&s,id,6,14,None);
   let row=s.section(id);
   json!({"id":id,"sequence":row.and_then(|v|v.sequence().ok().flatten()),"style":row.and_then(|v|v.style().ok().flatten()),"captureIntent":match result{Ok(_)=>"ok".into(),Err(e)=>format!("{e:?}")}})
  }).collect::<Vec<_>>())).unwrap_or(Value::Null)
 }).await.unwrap_or_else(|e|json!({"workerError":e.to_string()}));
    let dom = checked(desktop, include_str!("diagnostics.js"))
        .await
        .unwrap_or_else(|e| json!({"error":e}));
    json!({"checklist":runtime,"captureIntentFromOriginalSnapshot":original,"dom":dom})
}
