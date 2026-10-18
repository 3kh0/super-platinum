//! Short, closed renderer observations for the shared group intake. No DOM lease.
use super::*;
use std::time::Duration;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    stage: usize,
    fields: Vec<String>,
    lowered: Vec<String>,
    locale: String,
}
pub(super) struct Lowered {
    pub(super) generation: String,
    pub(super) fields: Vec<String>,
    pub(super) lowered: Vec<String>,
    pub(super) locale: Option<String>,
}
fn accept(
    generation: &str,
    stage: usize,
    fields: &[String],
    locale: &mut Option<String>,
    reply: Reply,
) -> Result<Vec<String>, Error> {
    if reply.generation != generation
        || reply.stage != stage
        || reply.fields != fields
        || reply.lowered.len() != fields.len()
        || reply.locale.is_empty()
        || locale
            .as_ref()
            .is_some_and(|locale| locale != &reply.locale)
    {
        return Err(unavailable());
    }
    *locale = Some(reply.locale);
    Ok(reply.lowered)
}
pub(super) async fn lower(
    state: Signal<ShellState>,
    current: impl Fn(&ShellState) -> bool,
    generation: &str,
    fields: &[String],
) -> Result<Lowered, Error> {
    if generation.is_empty() || fields.iter().any(String::is_empty) || !current(&state.peek()) {
        return Err(unavailable());
    }
    let mut locale = None;
    let mut lowered = Vec::with_capacity(fields.len());
    for (stage, batch) in fields.chunks(128).enumerate() {
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        let script = format!(
            "{}\nreturn groupSourceLower({});",
            include_str!("render.js"),
            serde_json::json!({"generation":generation,"stage":stage,"fields":batch})
        );
        // Pinned Eval::join consumes the short evaluation, as in the accepted
        // channel/member field renderer. No retained Eval or listener is needed.
        let reply = tokio::time::timeout(
            Duration::from_secs(5),
            dioxus::document::eval(&script).join::<Option<Reply>>(),
        )
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
        if !current(&state.peek()) {
            return Err(unavailable());
        }
        lowered.extend(accept(generation, stage, batch, &mut locale, reply)?);
    }
    if !current(&state.peek()) {
        return Err(unavailable());
    }
    Ok(Lowered {
        generation: generation.into(),
        fields: fields.to_vec(),
        lowered,
        locale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply() -> Reply {
        Reply {
            generation: "receipt".into(),
            stage: 0,
            fields: vec![" Group ".into()],
            lowered: vec![" group ".into()],
            locale: "en-US".into(),
        }
    }
    #[test]
    fn original_fields_generation_stage_and_locale_are_required() {
        for kind in 0..6 {
            let mut r = reply();
            match kind {
                0 => r.generation = "replacement".into(),
                1 => r.stage = 1,
                2 => r.fields[0] = "Group".into(),
                3 => r.lowered.clear(),
                4 => r.locale.clear(),
                _ => r.locale = "tr-TR".into(),
            }
            let mut locale = Some("en-US".into());
            assert!(accept("receipt", 0, &[" Group ".into()], &mut locale, r).is_err());
            assert_eq!(locale.as_deref(), Some("en-US"));
        }
    }
    #[test]
    fn raw_whitespace_and_observed_locale_are_preserved() {
        let mut locale = None;
        assert_eq!(
            accept("receipt", 0, &[" Group ".into()], &mut locale, reply()).unwrap(),
            vec![" group "]
        );
        assert_eq!(locale.as_deref(), Some("en-US"));
    }
}
