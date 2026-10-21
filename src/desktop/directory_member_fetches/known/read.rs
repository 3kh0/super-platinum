//! Known nzRy/xqiG read plan; no UFM placeholder or whole-attempt retry policy.
use super::*;
use crate::canvas::comments::editor_recovery_fixture as canvas_fixture;
use crate::profile_edit::sections::fixture::member_recovery as fixture;
use std::cell::Cell;
use super_platinum_core::{
    block_actions::directory_endpoint::{DirectoryEndpoint, member_info::project_interaction},
    slack::transport::{ChannelReadAttempt, ChannelReadResponse},
};

pub(super) struct Received {
    pub(super) requested: Vec<String>,
    pub(super) rows: Vec<Value>,
    pub(super) interactions: BTreeMap<String, bool>,
    pub(super) disconnected: Vec<String>,
    // Original ordered evidence remains alive throughout renderer/adoption.
    pub(super) pages: Vec<Arc<Page>>,
    pub(super) errors: Vec<Arc<Error>>,
}
struct Chunk {
    requested: Vec<String>,
    pages: Vec<Arc<Page>>,
    error: Option<Arc<Error>>,
    terminal: bool,
    cooldown: bool,
}
#[derive(Default)]
struct Collected {
    rows: Vec<Value>,
    interactions: BTreeMap<String, bool>,
    disconnected: Vec<String>,
    pages: Vec<Arc<Page>>,
    errors: Vec<Arc<Error>>,
    remaining: BTreeSet<String>,
    nonterminal: bool,
}
impl Collected {
    fn append(&mut self, chunks: Vec<Chunk>) -> Vec<String> {
        let mut pending = Vec::new();
        for chunk in chunks {
            self.nonterminal |= !chunk.terminal && !chunk.cooldown;
            if chunk.terminal {
                for id in &chunk.requested {
                    self.remaining.remove(id);
                }
            }
            if let Some(error) = chunk.error {
                if !chunk.cooldown {
                    self.errors.push(error);
                }
            } else {
                for page in &chunk.pages {
                    self.rows.extend_from_slice(page.rows());
                    self.interactions.extend(page.interactions().clone());
                    self.disconnected.extend_from_slice(page.disconnected_ids());
                    self.remaining.extend(page.pending_ids().iter().cloned());
                    pending.extend_from_slice(page.pending_ids());
                }
            }
            self.pages.extend(chunk.pages);
        }
        pending
    }
    fn finish(
        self,
        ids: &[String],
        prior: &BTreeMap<String, Option<bool>>,
    ) -> Result<Received, Error> {
        // Shape/local/unsent failures are not source empty terminals. A known
        // cooldown alone permits the already-completed, sent subset to settle.
        if self.nonterminal {
            return Err(unavailable());
        }
        let requested: Vec<_> = ids
            .iter()
            .filter(|id| !self.remaining.contains(*id))
            .cloned()
            .collect();
        let completed: BTreeSet<_> = requested.iter().map(String::as_str).collect();
        let rows: Vec<_> = self
            .rows
            .into_iter()
            .filter(|row| row["id"].as_str().is_some_and(|id| completed.contains(id)))
            .collect();
        let interactions: BTreeMap<_, _> = self
            .interactions
            .into_iter()
            .filter(|(id, _)| completed.contains(id.as_str()))
            .collect();
        let rows = project_interaction(&rows, &interactions, prior)?;
        let disconnected = self
            .disconnected
            .into_iter()
            .filter(|id| completed.contains(id.as_str()))
            .collect();
        Ok(Received {
            requested,
            rows,
            interactions,
            disconnected,
            pages: self.pages,
            errors: self.errors,
        })
    }
}
type Endpoint = tokio::sync::Mutex<Option<Arc<DirectoryEndpoint>>>;
pub(super) async fn acquire(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<Received, Error> {
    let mut collected = Collected {
        remaining: attempt.ids.iter().cloned().collect(),
        ..Default::default()
    };
    let mut ids = attempt.ids.clone();
    let endpoint = Endpoint::default();
    'rounds: for round in 0..4u64 {
        if !attempt.current(&state.peek()) || !current(&state.peek()) {
            return Err(unavailable());
        }
        if attempt.cooling(&state.peek()) {
            break;
        }
        let reads = ids
            .chunks(100)
            .map(|ids| attempt.read(ids))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pending = Vec::new();
        for pair in reads.chunks(2) {
            if !attempt.current(&state.peek()) || !current(&state.peek()) {
                return Err(unavailable());
            }
            if attempt.cooling(&state.peek()) {
                break 'rounds;
            }
            let (a, b) = tokio::join!(
                chunk(state, attempt, current, pair[0].clone(), &endpoint),
                async {
                    match pair.get(1) {
                        Some(read) => {
                            Some(chunk(state, attempt, current, read.clone(), &endpoint).await)
                        }
                        None => None,
                    }
                }
            );
            let chunks: Vec<_> = std::iter::once(a).chain(b).collect();
            let paused = chunks.iter().any(|chunk| chunk.cooldown);
            pending.extend(collected.append(chunks));
            if paused {
                break 'rounds;
            }
        }
        let mut seen = BTreeSet::new();
        ids = pending
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect();
        if ids.is_empty() {
            break;
        }
        if round == 3 {
            for id in &ids {
                collected.remaining.remove(id);
            }
            break;
        }
        if attempt.cooling(&state.peek()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500 * (round + 1))).await;
    }
    if !attempt.current(&state.peek()) || !current(&state.peek()) {
        return Err(unavailable());
    }
    collected.finish(&attempt.ids, attempt.observed.interactions())
}
struct Failure {
    error: Arc<Error>,
    cooldown: bool,
    terminal: bool,
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self {
            error: Arc::new(error),
            cooldown: false,
            terminal: false,
        }
    }
}
fn admit(
    shell: &ShellState,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
) -> Result<(), Failure> {
    if !attempt.current(shell) || !current(shell) {
        return Err(unavailable().into());
    }
    if attempt.cooling(shell) {
        return Err(Failure {
            error: Arc::new(unavailable()),
            cooldown: true,
            terminal: false,
        });
    }
    Ok(())
}
fn network_failure(
    mut state: Signal<ShellState>,
    attempt: &Attempt,
    error: Error,
    member: bool,
) -> Failure {
    attempt.failure(&mut state.write(), &error);
    let terminal = member
        && matches!(
            error,
            Error::Api(_) | Error::HttpStatus { .. } | Error::RateLimited { .. }
        );
    Failure {
        error: Arc::new(error),
        cooldown: false,
        terminal,
    }
}
fn before_send(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
    refused: &Cell<u8>,
) -> bool {
    let shell = state.peek();
    if !attempt.current(&shell) || !current(&shell) {
        refused.set(1);
        return false;
    }
    if attempt.cooling(&shell) {
        refused.set(2);
        return false;
    }
    true
}
fn read_failure(
    state: Signal<ShellState>,
    attempt: &Attempt,
    error: Error,
    member: bool,
    refused: u8,
) -> Failure {
    match refused {
        2 => Failure {
            error: Arc::new(error),
            cooldown: true,
            terminal: false,
        },
        1 => error.into(),
        _ => network_failure(state, attempt, error, member),
    }
}
async fn endpoint(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
    shared: &Endpoint,
) -> Result<Arc<DirectoryEndpoint>, Failure> {
    let mut stored = shared.lock().await;
    let ctx = &attempt.context;
    admit(&state.peek(), attempt, current)?;
    if let Some(endpoint) = stored
        .as_ref()
        .filter(|endpoint| endpoint.current(&ctx.session, Instant::now()))
    {
        return Ok(endpoint.clone());
    }
    let observed = Instant::now();
    let refused = Cell::new(0);
    let raw = if crate::fixture::is_fixture() {
        let request = DirectoryEndpoint::discovery(&ctx.client, &ctx.session)?;
        if fixture::enabled() {
            fixture::discovery(state, ctx, &request).await
        } else if canvas_fixture::enabled(&state.peek()) {
            canvas_fixture::discovery(state, &request).await
        } else {
            return Err(Error::Offline("closed member fixture".into()).into());
        }
    } else {
        let request = ChannelReadAttempt::discovery(&ctx.client, &ctx.session)?;
        match ctx
            .transport
            .execute_channel_read_current(request, || {
                before_send(state, attempt, current, &refused)
            })
            .await
        {
            Ok(ChannelReadResponse::Setup(raw)) => Ok(raw),
            Ok(_) => Err(unavailable()),
            Err(error) => Err(error),
        }
    }
    .map_err(|error| read_failure(state, attempt, error, false, refused.get()))?;
    if !attempt.current(&state.peek()) || !current(&state.peek()) {
        return Err(unavailable().into());
    }
    let endpoint = Arc::new(DirectoryEndpoint::decode(&ctx.session, &raw, observed)?);
    *stored = Some(endpoint.clone());
    Ok(endpoint)
}
async fn chunk(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
    read: Read,
    endpoint: &Endpoint,
) -> Chunk {
    let mut chunk = Chunk {
        requested: read.ids().to_vec(),
        pages: vec![],
        error: None,
        terminal: false,
        cooldown: false,
    };
    match pages(state, attempt, current, read, endpoint, &mut chunk.pages).await {
        Ok(()) => chunk.terminal = true,
        Err(failure) => {
            chunk.terminal = failure.terminal;
            chunk.cooldown = failure.cooldown;
            chunk.error = Some(failure.error);
        }
    }
    chunk
}
async fn pages(
    state: Signal<ShellState>,
    attempt: &Attempt,
    current: &impl Fn(&ShellState) -> bool,
    mut read: Read,
    shared: &Endpoint,
    out: &mut Vec<Arc<Page>>,
) -> Result<(), Failure> {
    let ctx = &attempt.context;
    let mut markers = BTreeSet::new();
    for _ in 0..50 {
        let endpoint = endpoint(state, attempt, current, shared).await?;
        admit(&state.peek(), attempt, current)?;
        let request = endpoint.member_info(&ctx.client, &ctx.session, Instant::now(), &read)?;
        let refused = Cell::new(0);
        let response = if crate::fixture::is_fixture() {
            if fixture::enabled() {
                fixture::member(state, ctx, &request).await
            } else if canvas_fixture::enabled(&state.peek()) {
                canvas_fixture::member(state, &request).await
            } else {
                Err(Error::Offline("closed member fixture".into()))
            }
        } else {
            ctx.transport
                .execute_directory_member_info_current(request, || {
                    before_send(state, attempt, current, &refused)
                })
                .await
                .map(|response| response.into_parts())
        }
        .map_err(|error| read_failure(state, attempt, error, true, refused.get()))?;
        // Already-issued sibling success survives a new shared429 cooldown.
        if !attempt.current(&state.peek())
            || !current(&state.peek())
            || !endpoint.current(&ctx.session, Instant::now())
        {
            return Err(unavailable().into());
        }
        let page = Arc::new(Page::decode(&read, response.0, response.1)?);
        let next = page.next_marker().map(str::to_owned);
        out.push(page);
        let Some(next) = next else {
            return Ok(());
        };
        if !markers.insert(next.clone()) {
            return Err(unavailable().into());
        }
        read = read.next_page(&next)?;
    }
    Err(unavailable().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const A: &str = "UKNOWN001";
    const B: &str = "UKNOWN002";
    const C: &str = "UKNOWN003";
    fn page(ids: &[&str], raw: Value) -> Arc<Page> {
        Arc::new(
            Page::decode(
                &Read::capture(ids.iter().map(|id| (*id).into()).collect()).unwrap(),
                raw,
                None,
            )
            .unwrap(),
        )
    }
    fn chunk(
        id: &str,
        pages: Vec<Arc<Page>>,
        error: Option<Arc<Error>>,
        terminal: bool,
        cooldown: bool,
    ) -> Chunk {
        Chunk {
            requested: vec![id.into()],
            pages,
            error,
            terminal,
            cooldown,
        }
    }
    fn collected(ids: &[&str]) -> Collected {
        Collected {
            remaining: ids.iter().map(|id| (*id).into()).collect(),
            ..Default::default()
        }
    }
    #[test]
    fn completed_success_and_sent_failure_survive_unsent_cooldown_without_placeholder() {
        let p = page(
            &[A],
            json!({"ok":true,"results":[{"id":A,"name":"Actual","profile":{}}],"can_interact":{(A):false},"disconnected_ids":[A]}),
        );
        let failed = Arc::new(Error::RateLimited {
            retry_after_secs: Some(30),
        });
        let mut all = collected(&[A, B, C]);
        all.append(vec![
            chunk(A, vec![p.clone()], None, true, false),
            chunk(B, vec![], Some(failed.clone()), true, false),
            chunk(C, vec![], Some(Arc::new(unavailable())), false, true),
        ]);
        let result = all
            .finish(&[A.into(), B.into(), C.into()], &BTreeMap::new())
            .unwrap();
        assert_eq!(result.requested, vec![A, B]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0]["is_stranger"], true);
        assert_eq!(result.disconnected, vec![A]);
        assert_eq!(result.interactions, BTreeMap::from([(A.into(), false)]));
        assert!(Arc::ptr_eq(&result.pages[0], &p));
        assert!(Arc::ptr_eq(&result.errors[0], &failed));
    }
    #[test]
    fn local_offline_or_shape_failure_cannot_grant_completed_scope() {
        for error in [Error::Offline("synthetic".into()), unavailable()] {
            let mut all = collected(&[A, B]);
            let p = page(&[A], json!({"ok":true,"results":[{"id":A,"profile":{}}]}));
            all.append(vec![
                chunk(A, vec![p], None, true, false),
                chunk(B, vec![], Some(Arc::new(error)), false, false),
            ]);
            assert!(
                all.finish(
                    &[A.into(), B.into()],
                    &BTreeMap::from([(A.into(), Some(true))])
                )
                .is_err()
            );
        }
    }
    #[test]
    fn pending_round_keeps_custody_until_completion_and_last_interaction_wins() {
        let first = page(
            &[A],
            json!({"ok":true,"results":[{"id":A,"profile":{}}],"can_interact":{(A):false},"pending_ids":[A]}),
        );
        let second = page(
            &[A],
            json!({"ok":true,"results":[{"id":A,"profile":{}}],"can_interact":{(A):true}}),
        );
        let mut all = collected(&[A]);
        assert_eq!(
            all.append(vec![chunk(A, vec![first], None, true, false)]),
            vec![A]
        );
        assert!(all.remaining.contains(A));
        all.append(vec![chunk(A, vec![second], None, true, false)]);
        assert!(!all.remaining.contains(A));
        let result = all.finish(&[A.into()], &BTreeMap::new()).unwrap();
        assert_eq!(result.requested, vec![A]);
        assert_eq!(result.rows.len(), 2);
        assert!(result.rows.iter().all(|row| row["is_stranger"] == false));
    }
    #[test]
    fn unfinished_pending_or_failed_page_chain_cannot_publish_earlier_partial_rows() {
        for terminal_error in [false, true] {
            let p = page(
                &[A],
                json!({"ok":true,"results":[{"id":A,"profile":{}}],"pending_ids":[A]}),
            );
            let mut all = collected(&[A]);
            all.append(vec![chunk(
                A,
                vec![p],
                terminal_error.then(|| Arc::new(Error::Api("synthetic_terminal".into()))),
                true,
                false,
            )]);
            let result = all.finish(&[A.into()], &BTreeMap::new()).unwrap();
            assert!(result.rows.is_empty());
            assert_eq!(result.requested.is_empty(), !terminal_error);
        }
    }
}
