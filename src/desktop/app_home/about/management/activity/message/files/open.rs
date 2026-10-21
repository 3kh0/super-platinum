use super::*;
use super_platinum_core::slack::models::File;

pub(super) struct Attempt {
    source: Source,
    observation: FileObservation,
    chain: u64,
    url: String,
    painted: title::Painted,
    part: usize,
}
impl Attempt {
    pub(super) fn id(&self) -> &str {
        self.source.id()
    }
    fn current(&self, s: &ShellState) -> bool {
        self.source.current(s)
            && self.observation.current(s)
            && self.painted.current(s, self.source.team())
            && state(s, self.source.visit()).is_some_and(|st| {
                st.navigation
                    .as_ref()
                    .is_some_and(|a| a.chain == self.chain)
            })
    }
    fn expected(&self) -> Value {
        let mut value = self.source.expected(self.chain);
        value.as_object_mut().unwrap().remove("channel");
        value["file"] = serde_json::json!({"id":self.id(),"kind":"list","label":self.painted.label,"url":self.url,"part":self.part,"fragments":self.painted.fragments});
        value
    }
}
pub(super) fn start(
    mut signal: Signal<ShellState>,
    source: Source,
    observation: FileObservation,
    url: String,
    painted: title::Painted,
    part: usize,
) {
    if !source.current(&signal.peek())
        || !observation.current(&signal.peek())
        || !painted.current(&signal.peek(), source.team())
        || state(&signal.peek(), source.visit())
            .is_none_or(|s| s.navigation.is_some() || s.pending.is_some())
    {
        return;
    }
    let attempt = Arc::new(Attempt {
        source,
        observation,
        chain: next(),
        url,
        painted,
        part,
    });
    dioxus::core::spawn_forever(async move {
        let expected = attempt.expected();
        let Some(initial) = navigation::native::lock(&expected).await else {
            return;
        };
        if !attempt.source.current(&signal.peek())
            || !attempt.observation.current(&signal.peek())
            || !attempt
                .painted
                .current(&signal.peek(), attempt.source.team())
        {
            initial.release();
            return;
        }
        let ctx = {
            let mut s = signal.write();
            if state(&s, attempt.source.visit()).is_none_or(|st| st.navigation.is_some()) {
                initial.release();
                return;
            }
            let ctx = Context::capture(&s, attempt.source.team());
            state_mut(&mut s, attempt.source.visit())
                .unwrap()
                .navigation = Some(attempt.clone());
            ctx
        };
        // Preserve exact buffers without holding a DOM lock through I/O.
        initial.release();
        let result = match ctx {
            Ok(ctx) => acquire::guarded(
                acquire::read(signal, &ctx, attempt.id(), "navigation", || {
                    attempt.current(&signal.peek()) && ctx.current(&signal.peek())
                }),
                || attempt.current(&signal.peek()) && ctx.current(&signal.peek()),
            )
            .await
            .unwrap_or_else(|| Err(Error::Offline("File navigation expired".into()))),
            Err(e) => Err(e),
        };
        if !attempt.current(&signal.peek()) {
            clear(signal, &attempt, None);
            return;
        }
        let raw = result.as_ref().ok().map(|v| v.raw().clone());
        let target = result.and_then(|received| {
            target(&received, attempt.source.team(), attempt.id(), &attempt.url)
        });
        let Some(final_lock) = navigation::native::lock(&expected).await else {
            clear(signal, &attempt, raw);
            return;
        };
        if !attempt.current(&signal.peek()) {
            final_lock.release();
            clear(signal, &attempt, raw);
            return;
        }
        match target {
            Ok(file) => {
                clear(signal, &attempt, raw);
                // This synchronous opener reaches the existing List shell/read
                // path. The original Activity document stays retained behind it.
                crate::files::open(signal, file);
                final_lock.release();
            }
            Err(error) => {
                let message = if matches!(error, Error::Offline(_)) {
                    "Connect to Slack to open this List."
                } else {
                    "Could not open this List. It may no longer be accessible. Try again."
                };
                navigation::native::feedback(&expected, message);
                clear(signal, &attempt, raw);
                final_lock.release();
            }
        }
    });
}
fn clear(mut signal: Signal<ShellState>, attempt: &Attempt, response: Option<Value>) {
    // Clear only the original pending slot; never transfer late feedback.
    let original = state(&signal.peek(), attempt.source.visit()).is_some_and(|st| {
        st.navigation
            .as_ref()
            .is_some_and(|a| a.chain == attempt.chain)
    });
    if !original {
        return;
    }
    let current = attempt.current(&signal.peek());
    if let Some(st) = state_mut(&mut signal.write(), attempt.source.visit()) {
        st.navigation = None;
        if current {
            st.last_navigation = Some(serde_json::json!({"id":attempt.id(),"response":response}));
        }
    }
}
fn target(received: &OrderedFileInfo, team: &str, id: &str, url: &str) -> Result<File, Error> {
    let unavailable = || Error::Api("activity_list_target_unavailable".into());
    if received.file()["id"].as_str() != Some(id) {
        return Err(unavailable());
    }
    // A fresh complete raw file supplies native routing facts. The title store
    // supplies only the separate, source-backed ordinary status/type check.
    let mut store = api::custom::texty::file_title_source::Store::new();
    store.ingest_info(team, received)?;
    let observed = store.row(id)?;
    let label = policy::label(observed.effective(), Kind::List).ok_or_else(unavailable)?;
    if label.url.as_deref() != Some(url)
        || policy::truth(&received.file()["is_deleted"])
        || policy::truth(&received.file()["product_icon"])
    {
        return Err(unavailable());
    }
    let file: File = serde_json::from_value(received.file().clone()).map_err(|_| unavailable())?;
    if file.id.as_deref() != Some(id) || !super_platinum_core::slack_lists::is_list(&file) {
        return Err(unavailable());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn raw() -> Value {
        json!({"ok":true,"file":{"id":"FLIST","mode":"list","filetype":"list","created":1,"title":"Original &amp; List","permalink":"https://fixture.invalid/list","access":"read","opaque":{"keep":[false,0,null]}}})
    }
    #[test]
    fn fresh_target_uses_exact_full_original_file_not_title_projection() {
        let raw = raw();
        let received = OrderedFileInfo::decode(raw.to_string().as_bytes()).unwrap();
        let file = target(&received, "T1", "FLIST", "https://fixture.invalid/list").unwrap();
        assert_eq!(file.extra["opaque"], raw["file"]["opaque"]);
        assert_eq!(file.extra["access"], "read");
        assert_eq!(file.title.as_deref(), Some("Original &amp; List"));
    }
    #[test]
    fn fresh_target_refuses_substitution_deleted_status_and_changed_permalink() {
        for change in 0..5 {
            let mut raw = raw();
            match change {
                0 => raw["file"]["id"] = json!("FOTHER"),
                1 => {
                    raw["file"]["mode"] = json!("quip");
                    raw["file"]["filetype"] = json!("quip");
                }
                2 => raw["file"]["is_deleted"] = json!(true),
                3 => raw["file"]["file_access"] = json!("access_denied"),
                _ => raw["file"]["permalink"] = json!("https://fixture.invalid/replaced"),
            };
            let received = OrderedFileInfo::decode(raw.to_string().as_bytes()).unwrap();
            assert!(
                target(&received, "T1", "FLIST", "https://fixture.invalid/list").is_err(),
                "{change}"
            );
        }
    }
}
