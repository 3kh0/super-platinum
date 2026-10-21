//! Closed Add-to-channel endpoint. Publication and Add ledgers remain separate.
use super::*;
pub(crate) mod raw;
mod wire;
pub(crate) use wire::{mutation, read};
pub(crate) fn requested() -> bool {
    super::requested()
        && matches!(
            case().as_str(),
            "first-add-no-selection"
                | "first-add-success"
                | "first-add-duplicate"
                | "first-add-rejected"
                | "first-add-retirement"
                | "first-add-unavailable"
                | "first-tab-success"
                | "first-tab-empty"
                | "first-tab-failure"
                | "first-tab-retirement"
                | "first-tab-rate-limit"
                | "first-details-success"
                | "first-details-errors"
                | "first-details-close-held"
                | "first-details-supersession"
                | "first-details-account-retirement"
                | "first-details-rate-limit"
        )
}
/// Normalize only the new synthetic Add scene before any Home/About owner exists.
pub(crate) fn install_identity(s: &mut ShellState) {
    if !requested() {
        return;
    }
    let old = s
        .core
        .active_channel
        .clone()
        .expect("Original fixture channel");
    assert_eq!(old, "C2");
    let id = "CAPPHOME1";
    let w = s.core.workspaces.get_mut("TWORKFLOW").unwrap();
    let mut channel = w.channels.remove(&old).unwrap();
    channel.id = id.into();
    assert!(w.channels.insert(id.into(), channel).is_none());
    if let Some(messages) = w.messages.remove(&old) {
        w.messages.insert(id.into(), messages);
    }
    for order in [
        &mut w.starred_order,
        &mut w.dm_order,
        &mut w.recent_channels,
    ] {
        for value in order {
            if value == &old {
                *value = id.into();
            }
        }
    }
    if w.last_active_channel.as_ref() == Some(&old) {
        w.last_active_channel = Some(id.into());
    }
    s.core.active_channel = Some(id.into());
    for target in s.surfaces.values_mut() {
        if target.channel == old {
            target.channel = id.into();
        }
    }
    for channel in &mut s.channels {
        if channel.id == old {
            channel.id = id.into();
        }
    }
}
#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Data>>);
struct Data {
    requests: Vec<Value>,
    responses: Vec<Value>,
    settlements: Vec<Value>,
    releases: Vec<Value>,
    held: Vec<(usize, oneshot::Sender<()>)>,
    bookmarks: Vec<Value>,
}
impl Model {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(Data {
            requests: vec![],
            responses: vec![],
            settlements: vec![],
            releases: vec![],
            held: vec![],
            bookmarks: vec![],
        })))
    }
    pub(crate) fn snapshot(&self) -> Value {
        let m = self.0.lock().unwrap();
        json!({"requests":m.requests,"responses":m.responses,"settlements":m.settlements,"releases":m.releases,
            "held":m.held.iter().map(|(n,_)|n).collect::<Vec<_>>(),"bookmarks":m.bookmarks,"networkCalls":0})
    }
    pub(crate) fn release(&self, serial: usize) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        let i = m
            .held
            .iter()
            .position(|(n, _)| *n == serial)
            .ok_or("Original Add sender absent")?;
        let (_, tx) = m.held.remove(i);
        let retired = tx.is_closed();
        let v = json!({"serial":serial,"receiverRetired":retired,"delivered":tx.send(()).is_ok()});
        m.releases.push(v.clone());
        Ok(v)
    }
}
pub(crate) fn capture(s: &ShellState) -> Result<Model, String> {
    if !requested() {
        return Err("Closed Add case required".into());
    }
    let parent = super::capture(s)?;
    let m = parent.0.lock().unwrap();
    m.confirmation_add
        .clone()
        .ok_or("Original Add model absent".into())
}
struct Settlement {
    model: Model,
    serial: usize,
    returned: bool,
}
impl Drop for Settlement {
    fn drop(&mut self) {
        self.model
            .0
            .lock()
            .unwrap()
            .settlements
            .push(json!({"serial":self.serial,"returned":self.returned}));
    }
}
async fn deliver(
    model: Model,
    serial: usize,
    hold: bool,
    result: Result<Value, Error>,
) -> Result<Value, Error> {
    let rx = {
        let mut m = model.0.lock().unwrap();
        m.responses
            .push(json!({"serial":serial,"result":super::receipt(&result)}));
        if hold {
            let (tx, rx) = oneshot::channel();
            m.held.push((serial, tx));
            Some(rx)
        } else {
            None
        }
    };
    let mut settled = Settlement {
        model,
        serial,
        returned: false,
    };
    if let Some(rx) = rx {
        tokio::time::timeout(Duration::from_secs(45), rx)
            .await
            .map_err(|_| closed())?
            .map_err(|_| closed())?;
    }
    settled.returned = true;
    result
}
