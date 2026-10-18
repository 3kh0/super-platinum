use super::*;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    stage: usize,
    fields: Vec<String>,
    values: Vec<String>,
}
/// Each eval returns immediately. join observes completion; there is no retained
/// JS lease, shared document state, locale-sensitive conversion or native lock.
pub(super) async fn fields(
    state: Signal<ShellState>,
    owner: &Owner,
    generation: &str,
    fields: &[String],
) -> Result<Vec<String>, Error> {
    fields_guarded(state, |shell| owner.current(shell), generation, fields).await
}
pub(crate) async fn fields_guarded(
    state: Signal<ShellState>,
    current: impl Fn(&ShellState) -> bool,
    generation: &str,
    fields: &[String],
) -> Result<Vec<String>, Error> {
    let mut result = Vec::with_capacity(fields.len());
    for (stage, batch) in fields.chunks(128).enumerate() {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        let request = serde_json::json!({"generation":generation,"stage":stage,"fields":batch});
        let script = format!(
            "{}\nreturn channelOrdinaryFields({});",
            include_str!("render.js"),
            request
        );
        let reply = tokio::time::timeout(
            Duration::from_secs(5),
            dioxus::document::eval(&script).join::<Option<Reply>>(),
        )
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
        if !current(&state.peek())
            || reply.generation != generation
            || reply.stage != stage
            || reply.fields != batch
            || reply.values.len() != batch.len()
        {
            return Err(unavailable());
        }
        result.extend(reply.values);
    }
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(result)
}
