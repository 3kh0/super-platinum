use super::*;
pub(super) async fn snapshot(expected: &Expected) -> Result<Dates, Error> {
    let encoded = serde_json::to_string(expected).map_err(|_| unavailable())?;
    let script = concat!(
        include_str!("date_witness.js"),
        "\n",
        include_str!("native.js")
    );
    let mut eval = dioxus::document::eval(&format!(
        "{script}\nawait workflowActivityRead({encoded},dioxus);"
    ));
    // The pinned desktop query is GC-owned. Keep JS alive until Rust receives
    // its message; release on success, refusal, timeout or task cancellation.
    let _release = crate::app_home::inputs::native::retain(eval);
    tokio::time::timeout(Duration::from_secs(3), eval.recv::<Option<Dates>>())
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)
}
