use super::*;
use std::future::Future;

pub(super) async fn guarded<T>(
    future: impl Future<Output = T>,
    current: impl Fn() -> bool,
) -> Option<T> {
    tokio::pin!(future);
    let timeout = tokio::time::sleep(Duration::from_secs(45));
    tokio::pin!(timeout);
    loop {
        if !current() {
            return None;
        }
        tokio::select! {biased; v=&mut future=>return current().then_some(v), _=&mut timeout=>return None, _=tokio::time::sleep(Duration::from_millis(100))=>{}}
    }
}
pub(super) async fn read(
    state: Signal<ShellState>,
    ctx: &Context,
    id: &str,
    purpose: &str,
    current: impl Fn() -> bool,
) -> Result<OrderedFileInfo, Error> {
    let request = api::file_info(&ctx.client, &ctx.session, id);
    if crate::fixture::is_fixture() {
        super::super::super::fixture::file::read(state, &request, purpose).await
    } else {
        ctx.transport
            .execute_file_info_current(request, current)
            .await
    }
}
fn occurrences(s: &ShellState, visit: u64) -> Vec<Source> {
    let Some(d) = document(s, visit) else {
        return vec![];
    };
    let Some(detail) = &d.activity.detail else {
        return vec![];
    };
    let messages = detail
        .trigger_message()
        .map(|m| (Location::Trigger, m))
        .into_iter()
        .chain(
            detail
                .steps()
                .iter()
                .enumerate()
                .map(|(i, step)| (Location::Step(i), &step["outcome_message"])),
        );
    let mut found = BTreeMap::new();
    for (location, message) in messages {
        for (part, token) in presentation::parts(message).into_iter().enumerate() {
            let (id, kind) = match token {
                presentation::Part::Canvas(id) => (id, Kind::Canvas),
                presentation::Part::List(id) => (id, Kind::List),
                _ => continue,
            };
            if found.len() >= 64 {
                break;
            }
            if let Some(source) = Source::capture_file(s, visit, location, part, &id, kind) {
                found.entry(id).or_insert(source);
            }
        }
    }
    found.into_values().collect()
}
pub(in crate::app_home::about::management::activity) fn request(
    mut signal: Signal<ShellState>,
    visit: u64,
) {
    let candidates = occurrences(&signal.peek(), visit);
    if candidates.is_empty() {
        return;
    }
    let detail = candidates[0].detail_revision();
    let reset = state(&signal.peek(), visit).is_some_and(|s| s.detail != detail);
    if reset {
        if let Some(st) = state_mut(&mut signal.write(), visit) {
            *st = State {
                detail,
                ..Default::default()
            };
        }
    }
    // Retired pending reads never block a later explicit action. No late notice
    // or response can be attached to the replacement occurrence.
    let retired = state(&signal.peek(), visit)
        .and_then(|st| st.pending.as_ref().and_then(|(id, _)| st.entries.get(id)))
        .is_some_and(|e| !e.source.current(&signal.peek()));
    if retired {
        if let Some(st) = state_mut(&mut signal.write(), visit) {
            if let Some((id, _)) = st.pending.take() {
                if let Some(e) = st.entries.get_mut(&id) {
                    e.phase = "retired";
                }
            }
        }
    }
    if state(&signal.peek(), visit).is_none_or(|s| s.pending.is_some() || s.navigation.is_some()) {
        return;
    }
    for source in candidates {
        if state(&signal.peek(), visit).is_some_and(|s| s.entries.contains_key(source.id())) {
            continue;
        }
        let observation = FileObservation::capture(&signal.peek(), source.team(), source.id());
        if let Ok(observation) = observation {
            if let Some(st) = state_mut(&mut signal.write(), visit) {
                st.entries.insert(
                    source.id().into(),
                    Entry {
                        phase: "ready",
                        source,
                        receipt: Some(observation.receipt().clone()),
                        notice: None,
                    },
                );
            }
            continue;
        }
        start(signal, source);
        return;
    }
}
pub(super) fn retry(signal: Signal<ShellState>, source: Source) {
    if !source.current(&signal.peek())
        || state(&signal.peek(), source.visit())
            .is_none_or(|s| s.pending.is_some() || s.navigation.is_some())
    {
        return;
    }
    // Explicit retry is a fresh original occurrence; no automatic retry loop.
    start(signal, source);
}
fn start(mut signal: Signal<ShellState>, source: Source) {
    let visit = source.visit();
    let id = source.id().to_owned();
    let chain = next();
    let captures = {
        let mut s = signal.write();
        if !source.current(&s) {
            return;
        }
        let ctx = Context::capture(&s, source.team());
        let read = FileInfoRead::capture(&mut s, source.team(), &id);
        let Some(st) = state_mut(&mut s, visit) else {
            return;
        };
        st.entries.insert(
            id.clone(),
            Entry {
                phase: "loading",
                source: source.clone(),
                receipt: None,
                notice: None,
            },
        );
        st.pending = Some((id.clone(), chain));
        (ctx, read)
    };
    dioxus::core::spawn_forever(async move {
        let current = || {
            source.current(&signal.peek())
                && state(&signal.peek(), visit)
                    .is_some_and(|s| s.pending.as_ref() == Some(&(id.clone(), chain)))
        };
        let (ctx, read) = match captures {
            (Ok(ctx), Ok(read)) => (ctx, read),
            _ => {
                finish(
                    signal,
                    &source,
                    chain,
                    None,
                    Err(Error::Offline("File details unavailable".into())),
                );
                return;
            }
        };
        let result = guarded(
            self::read(signal, &ctx, &id, "metadata", || {
                current() && ctx.current(&signal.peek()) && read.current(&signal.peek())
            }),
            || current() && ctx.current(&signal.peek()) && read.current(&signal.peek()),
        )
        .await;
        if !current() {
            return;
        }
        let received =
            result.unwrap_or_else(|| Err(Error::Offline("File details read expired".into())));
        let raw = received.as_ref().ok().map(|r| r.raw().clone());
        let published = received.and_then(|received| read.publish(&mut signal.write(), &received));
        finish(signal, &source, chain, raw, published);
        request(signal, visit);
    });
}
fn finish(
    mut signal: Signal<ShellState>,
    source: &Source,
    chain: u64,
    receipt: Option<Value>,
    result: Result<(), Error>,
) {
    let mut s = signal.write();
    if !source.current(&s) {
        return;
    }
    let Some(st) = state_mut(&mut s, source.visit()) else {
        return;
    };
    if st.pending.as_ref() != Some(&(source.id().into(), chain)) {
        return;
    }
    st.pending = None;
    if let Some(entry) = st.entries.get_mut(source.id()) {
        entry.phase = if result.is_ok() {
            "ready"
        } else {
            "unavailable"
        };
        entry.notice = match result {
            Ok(()) => None,
            Err(Error::Offline(_)) => {
                Some("Connect to Slack to load file details. Retry file details when connected.")
            }
            Err(_) => Some("File details could not load. Retry file details to try again."),
        };
        entry.receipt = receipt;
    }
}
