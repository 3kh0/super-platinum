use super::*;
use tokio::sync::oneshot;
pub(in crate::canvas) const PREVIEW: &str = "https://example.test/canonical/preview.svg";
#[derive(Default, Clone, Copy)]
pub(in crate::canvas) enum ResolveMode {
    #[default]
    Success,
    Held,
}
#[derive(Default)]
struct Reads {
    next: ResolveMode,
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Option<(usize, oneshot::Sender<()>)>,
}
static READS: OnceLock<Mutex<Reads>> = OnceLock::new();
pub(in crate::canvas) fn configure(shell: &ShellState, mode: ResolveMode) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(Error::Api("canonical_fixture_retired".into()));
    }
    READS.get_or_init(Default::default).lock().unwrap().next = mode;
    Ok(())
}
pub(in crate::canvas) fn snapshot() -> Value {
    let record = READS.get_or_init(Default::default).lock().unwrap();
    json!({"requests":record.requests,"responses":record.responses,"settlements":record.settlements,
        "releases":record.releases,"held":record.held.as_ref().map(|(serial,_)|serial)})
}
pub(in crate::canvas) fn release(shell: &ShellState) -> Result<Value, Error> {
    if !enabled(shell) {
        return Err(Error::Api("canonical_fixture_retired".into()));
    }
    let mut record = READS.get_or_init(Default::default).lock().unwrap();
    let (serial, sender) = record
        .held
        .take()
        .ok_or_else(|| Error::Api("canonical_no_held_read".into()))?;
    let delivered = sender.send(()).is_ok();
    let proof = json!({"serial":serial,"delivered":delivered});
    record.releases.push(proof.clone());
    Ok(proof)
}
pub(in crate::canvas) async fn resolve(
    state: Signal<ShellState>,
    id: &str,
) -> Result<Value, Error> {
    let wire = {
        let shell = state.peek();
        if !enabled(&shell) || !matches!(id, HOSTED | CANVAS | LIST) {
            return Err(Error::Api("canonical_fixture_target".into()));
        }
        let session = super_platinum_core::config::WorkspaceSession {
            team_id: shell
                .core
                .active_team
                .clone()
                .ok_or_else(|| Error::Api("canonical_fixture_team".into()))?,
            enterprise_id: None,
            user_id: shell.self_account.user_id.clone(),
            name: "Canonical fixture".into(),
            url: "https://fixture.slack.com".into(),
            token: "fixture-only".into(),
        };
        let request = api::file_info(&shell.core.client, &session, id);
        let super_platinum_core::slack::client::RequestBody::Form(fields) = request.body else {
            return Err(Error::Api("canonical_fixture_form".into()));
        };
        json!({"method":request.method,"url":request.url,"fields":fields.into_iter().filter(|(k,_)|k!="token").collect::<Vec<_>>()})
    };
    let target = if destinations_requested() {
        match destination_id().filter(|target| *target == id) {
            Some(CANVAS) => destination_canvas::file(),
            Some(LIST) => crate::slack_lists::fixture::canvas_navigation::file(),
            _ => return Err(Error::Api("canonical_fixture_wrong_destination".into())),
        }
    } else {
        let mut target = file(id);
        target["title"] = json!("Fresh hosted preview");
        target["mimetype"] = json!("image/svg+xml");
        target["url_private"] = json!(PREVIEW);
        target.as_object_mut().unwrap().remove("thumb_360");
        target
    };
    let envelope = json!({"ok":true,"file":target});
    let (serial, receiver) = {
        let mut record = READS.get_or_init(Default::default).lock().unwrap();
        let serial = record.requests.len() + 1;
        record.requests.push(wire);
        record.responses.push(envelope.clone());
        let receiver = if matches!(std::mem::take(&mut record.next), ResolveMode::Held) {
            if record.held.is_some() {
                return Err(Error::Api("canonical_fixture_duplicate_held".into()));
            }
            let (sender, receiver) = oneshot::channel();
            record.held = Some((serial, sender));
            Some(receiver)
        } else {
            None
        };
        (serial, receiver)
    };
    if let Some(receiver) = receiver {
        tokio::time::timeout(std::time::Duration::from_secs(45), receiver)
            .await
            .map_err(|_| Error::Api("canonical_fixture_read_timeout".into()))?
            .map_err(|_| Error::Api("canonical_fixture_read_cancelled".into()))?;
    }
    READS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .settlements
        .push(json!({"serial":serial,"delivered":true}));
    Ok(envelope)
}
