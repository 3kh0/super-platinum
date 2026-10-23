//! Owned in-memory socket and route observations for closed Share cases.
//! This does not create a network connection or claim a successful HTTP read.
use super::*;
use std::time::Instant;
use super_platinum_core::{slack::realtime::Connection, state::RealtimeStatus};
use tokio::sync::mpsc;

pub(super) struct Data {
    owner: Value,
    generation: u64,
    installed: Value,
    // Keep the closed socket carrier alive for the original fixture lifetime.
    // No task drains this queue into a network, and no protocol response is
    // manufactured from a queued frame.
    receiver: mpsc::Receiver<String>,
}
impl Data {
    pub(super) fn snapshot(&self) -> Value {
        json!({"owner":self.owner,"generation":self.generation,
            "installed":self.installed,"carrier":"owned-in-memory-realtime-channel",
            "receiverClosed":self.receiver.is_closed(),"queuedFrames":self.receiver.len(),
            "networkCalls":0,"httpSuccessClaimed":false})
    }
}

/// Explicit fixture source transition, never an invented realtime event. The
/// original carrier/owner remains retained; only actual route and health change.
pub(crate) fn offline(s: &mut ShellState) -> Result<Value, String> {
    if !(search::empty::requested() && search::empty::case() == "warm-offline")
        && !startup::requested()
    {
        return Err("Named warm-offline Share case required".into());
    }
    let model = capture(s)?;
    let (_, session) = context(s).map_err(|_| "Original Share offline owner missing")?;
    let m = model.0.lock().unwrap();
    let d = m
        .connectivity
        .as_ref()
        .ok_or("Original closed connection absent")?;
    let current_generation = s.core.workspaces[&session.team_id].rt_generation;
    let retired_startup = startup::requested()
        && startup::case() == "socket-retired"
        && current_generation == d.generation.wrapping_add(1)
        && m.startup.as_ref().is_some_and(|v| {
            let proof = v.snapshot();
            proof["held"] == json!([])
                && proof["settlements"]
                    .as_array()
                    .is_some_and(|r| r.len() == 2)
        });
    if d.owner != owner(s, &session)
        || (d.generation != current_generation && !retired_startup)
        || d.receiver.is_closed()
    {
        return Err("Original Share connection retired".into());
    }
    let before = json!({"routable":s.connection.routable,"routeObserved":s.connection.routed_at.is_some(),"realtimeConnected":s.core.workspaces[&session.team_id].rt.is_connected()});
    s.connection.routable = false;
    s.connection.routed_at = Some(Instant::now());
    s.core
        .transport
        .as_ref()
        .ok_or("Original transport absent")?
        .set_health(super_platinum_core::slack::transport::Health::Offline);
    let mut proof = json!({"kind":"closed-fixture-connectivity-transition","owner":d.owner,
        "generation":d.generation,"before":before,"after":{"routable":false,"routeObserved":true,"realtimeConnected":true,"transportHealth":"offline"},"networkCalls":0,"eventFabricated":false});
    if startup::requested() {
        proof["currentGeneration"] = json!(current_generation);
    }
    Ok(proof)
}

pub(crate) fn install(s: &mut ShellState) -> Result<(), String> {
    if !requested() {
        return Ok(());
    }
    if !crate::fixture::is_fixture() || !s.signed_in || s.loading {
        return Err("Original signed-in closed Share fixture required".into());
    }
    let model = capture(s)?;
    let (_, session) = context(s).map_err(|_| "Original Share connectivity owner missing")?;
    let identity = owner(s, &session);
    let transport = s
        .core
        .transport
        .as_ref()
        .ok_or("Original transport absent")?;
    if transport.health() == super_platinum_core::slack::transport::Health::Offline {
        return Err("Share fixture transport was already offline".into());
    }
    let mut m = model.0.lock().unwrap();
    if m.connectivity.is_some() {
        return Err("Original Share connectivity already installed".into());
    }
    let (tx, receiver) = mpsc::channel(64);
    let workspace = s
        .core
        .workspaces
        .get_mut(&session.team_id)
        .ok_or("Original Share workspace absent")?;
    workspace.rt = RealtimeStatus::Connected(Connection::from_sender(tx));
    let generation = workspace.rt_generation;
    s.connection.routable = true;
    s.connection.routed_at = Some(Instant::now());
    if search::empty::requested() && search::empty::case() == "cold-offline" {
        s.connection.routable = false;
        transport.set_health(super_platinum_core::slack::transport::Health::Offline);
    }
    m.connectivity = Some(Data {
        owner: identity,
        generation,
        installed: json!({"routeObserved":true,"routable":s.connection.routable,"realtimeConnected":true,
            "transportHealth":match transport.health(){
                super_platinum_core::slack::transport::Health::Unknown=>"unknown",
                super_platinum_core::slack::transport::Health::Online=>"online",
                super_platinum_core::slack::transport::Health::Offline=>"offline"}}),
        receiver,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_socket_has_a_live_owned_receiver_but_no_network_sink() {
        let (tx, receiver) = mpsc::channel(2);
        let connection = Connection::from_sender(tx);
        connection.try_send("original local frame".into()).unwrap();
        let data = Data {
            owner: json!({"account":"fixture","team":"TWORKFLOW","user":"UWORKFLOW"}),
            generation: 3,
            installed: json!({"routeObserved":true,"routable":true,"realtimeConnected":true,"transportHealth":"unknown"}),
            receiver,
        };
        let receipt = data.snapshot();
        assert_eq!(receipt["receiverClosed"], false);
        assert_eq!(receipt["queuedFrames"], 1);
        assert_eq!(receipt["networkCalls"], 0);
        assert_eq!(receipt["httpSuccessClaimed"], false);
        assert_eq!(receipt["installed"]["transportHealth"], "unknown");
        drop(data);
        assert!(connection.try_send("must not escape".into()).is_err());
    }
}
