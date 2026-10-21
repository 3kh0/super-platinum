//! Leaving a direct entry must not discard a native edit awaiting IPC.
use super::*;

fn expected(s: &ShellState, visit: u64) -> Option<Value> {
    if !showing(s) {
        return None;
    }
    let h = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .filter(|h| h.id == visit)?;
    let d = document(s, visit);
    let mut fields = serde_json::Map::new();
    let activity = activity::navigation_fields(s, visit);
    if let Some(a) = &activity {
        for key in ["status", "limit", "user"] {
            fields.insert(
                format!("workflow-activity-{key}"),
                a["filters"][key].clone(),
            );
        }
        fields.insert("workflow-activity-user-search".into(), a["search"].clone());
    } else if let Some(d) = d {
        fields.insert("workflow-management-title".into(), json!(d.title));
        fields.insert(
            "workflow-management-description".into(),
            json!(d.description),
        );
    }
    Some(
        json!({"visit":h.id,"document":d.map_or(0,|d|d.id),"revision":d.map_or(0,|d|d.revision),"fields":fields,"activity":activity}),
    )
}
pub(in crate::app_home::about::management) fn leave(mut state: Signal<ShellState>, visit: u64) {
    let Some(captured) = expected(&state.peek(), visit) else {
        return;
    };
    dioxus::core::spawn_forever(async move {
        let script = concat!(
            include_str!("../activity/date_witness.js"),
            "\n",
            include_str!("back.js")
        );
        let mut eval = dioxus::document::eval(&format!(
            "{script}\nawait workflowEntryBack({captured},dioxus);"
        ));
        let release = crate::app_home::inputs::native::retain(eval);
        if matches!(
            tokio::time::timeout(Duration::from_secs(3), eval.recv::<bool>()).await,
            Ok(Ok(true))
        ) {
            if expected(&state.peek(), visit).as_ref() == Some(&captured) {
                close(&mut state.write(), visit);
            }
        }
        release.release();
    });
}
