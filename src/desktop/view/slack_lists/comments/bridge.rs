//! Retained exact-node capture. DOM IDs locate a mount once; they never authorize
//! a later draft. The original controller retains each capture before ACKing it.
use crate::{
    slack_lists::comments::draft::{self, Binding},
    state::ShellState,
};
use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use super_platinum_core::slack::Error;
use tokio::sync::{mpsc, oneshot};

const SCRIPT: &str = include_str!("bridge.js");
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
fn unavailable() -> Error {
    Error::Api("list_comment_native_capture_unavailable".into())
}

#[derive(Clone)]
pub(crate) struct Mounted {
    mount: String,
    scope: String,
    generation: u64,
    sequence: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
    sender: mpsc::UnboundedSender<Command>,
}
impl Mounted {
    pub(crate) fn mount(&self) -> &str {
        &self.mount
    }
    pub(crate) fn matches(&self, binding: &Binding) -> bool {
        self.scope == binding.scope() && self.generation == binding.generation()
    }
    pub(crate) fn failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }
}
struct Command {
    payload: Value,
    reply: Option<oneshot::Sender<Result<Value, Error>>>,
}
#[derive(Deserialize)]
struct Capture {
    scope: String,
    mount: String,
    sequence: u64,
    request: Option<String>,
    blocks: Vec<Value>,
    revision: u64,
    composing: bool,
    generation: u64,
    detached: bool,
    #[serde(rename = "finalCapture")]
    final_capture: bool,
}

pub(crate) fn mount(mut state: Signal<ShellState>, binding: Binding) {
    let (sender, commands) = mpsc::unbounded_channel();
    let mounted = Mounted {
        mount: uuid::Uuid::new_v4().to_string(),
        scope: binding.scope(),
        generation: binding.generation(),
        sequence: Arc::new(AtomicU64::new(0)),
        failed: Arc::new(AtomicBool::new(false)),
        sender,
    };
    let installed = {
        let mut shell = state.write();
        binding.visible_mount(&shell)
            && draft::install_native(&mut shell, &binding, mounted.clone())
    };
    if !installed {
        return;
    }
    launch(state, binding, mounted, commands, false);
}

/// Recover the privately retained original node after a failed ACK/eval. A
/// replacement with the same DOM id is never adopted by this operation.
pub(crate) fn retry(mut state: Signal<ShellState>, binding: Binding) -> bool {
    let (sender, commands) = mpsc::unbounded_channel();
    let mounted = {
        let shell = state.peek();
        if !binding.visible_mount(&shell) && !binding.retained_mount(&shell) {
            return false;
        }
        let Some(old) =
            draft::native(&shell, &binding).filter(|m| m.matches(&binding) && m.failed())
        else {
            return false;
        };
        Mounted {
            sender,
            failed: Arc::new(AtomicBool::new(false)),
            ..old
        }
    };
    if !draft::install_native(&mut state.write(), &binding, mounted.clone()) {
        return false;
    }
    launch(state, binding, mounted, commands, true);
    true
}

fn launch(
    mut state: Signal<ShellState>,
    binding: Binding,
    mounted: Mounted,
    mut commands: mpsc::UnboundedReceiver<Command>,
    recover: bool,
) {
    dioxus::core::spawn_forever(async move {
        let options = json!({"id":"slack-list-comment-draft","scope":mounted.scope,
            "generation":mounted.generation,"mount":mounted.mount,"recover":recover});
        let mut eval = dioxus::document::eval(&format!(
            "{SCRIPT}\nreturn await listCommentsComposeBridge({options},dioxus);"
        ));
        let mut completion = Box::pin(eval.join::<Value>());
        let mut pending: Option<(String, Command)> = None;
        let mut final_sequence = None;
        let mut finish_reply = None;
        let mut completed = false;
        let result:Result<(),Error>=async {
            loop {
                // Eval completion and the last message can become ready in either
                // order. A successful finish requires both before native_done.
                if completed && pending.as_ref().is_none_or(|(_,c)|c.payload["kind"]!="finish") { return Ok(()); }
                tokio::select! {
                    done=&mut completion, if !completed => {
                        let value=done.map_err(|_|unavailable())?;
                        if value["scope"]!=mounted.scope || value["mount"]!=mounted.mount || value["final"]!=true
                            || value["sequence"].as_u64()!=final_sequence || final_sequence.is_none() { return Err(unavailable()); }
                        completed=true;
                    }
                    command=commands.recv(), if pending.is_none() && finish_reply.is_none() && !completed => {
                        let Some(mut command)=command else { return Err(unavailable()); };
                        let request=uuid::Uuid::new_v4().to_string();
                        command.payload["scope"]=json!(mounted.scope);command.payload["mount"]=json!(mounted.mount);
                        command.payload["request"]=json!(request);
                        eval.send(command.payload.clone()).map_err(|_|unavailable())?;
                        pending=Some((request,command));
                    }
                    message=eval.recv::<Value>(), if final_sequence.is_none() || pending.as_ref().is_some_and(|(_,c)|c.payload["kind"]=="finish") => {
                        let value=message.map_err(|_|unavailable())?;
                        if value["scope"]!=mounted.scope || value["mount"]!=mounted.mount { return Err(unavailable()); }
                        match value["kind"].as_str() {
                            Some("capture") => {
                                let capture:Capture=serde_json::from_value(value.clone()).map_err(|_|unavailable())?;
                                if capture.scope!=mounted.scope || capture.mount!=mounted.mount || capture.generation!=mounted.generation
                                    || capture.sequence<=mounted.sequence.load(Ordering::SeqCst)
                                    || capture.sequence>9_007_199_254_740_991 || capture.revision>9_007_199_254_740_991 { return Err(unavailable()); }
                                draft::retain_capture(&mut state.write(),&binding,&mounted.mount,capture.sequence,
                                    capture.blocks,capture.composing,capture.detached)?;
                                mounted.sequence.store(capture.sequence,Ordering::SeqCst);
                                draft::save(state,binding.id().to_owned());
                                eval.send(json!({"kind":"ack","scope":mounted.scope,"mount":mounted.mount,"sequence":capture.sequence})).map_err(|_|unavailable())?;
                                if capture.detached||capture.final_capture { final_sequence=Some(capture.sequence); }
                                if let Some(request)=capture.request {
                                    if pending.as_ref().map(|p|p.0.as_str())!=Some(request.as_str()) { return Err(unavailable()); }
                                    let (_,command)=pending.take().unwrap();
                                    let delivered=command.reply.is_some_and(|reply|reply.send(Ok(value.clone())).is_ok());
                                    if !delivered && value["locked"]==true {
                                        // A cancelled awaiting caller cannot strand an admitted native lock.
                                        let _=mounted.sender.send(Command{payload:json!({"kind":"release","lease":request,"sequence":capture.sequence}),reply:None});
                                    }
                                }
                            }
                            Some("capture-refused"|"release-result") => {
                                let request=value["request"].as_str().ok_or_else(unavailable)?;
                                if pending.as_ref().map(|p|p.0.as_str())!=Some(request) { return Err(unavailable()); }
                                let (_,command)=pending.take().unwrap();
                                if command.payload["kind"]=="finish" && value["ok"]==true && value["finished"]==true {
                                    finish_reply=command.reply.map(|reply|(reply,value));
                                } else if let Some(reply)=command.reply { let _=reply.send(Ok(value)); }
                            }
                            _=>return Err(unavailable()),
                        }
                    }
                }
            }
        }.await;
        if let Some((_, command)) = pending {
            if let Some(reply) = command.reply {
                let _ = reply.send(Err(unavailable()));
            }
        }
        mounted.failed.store(result.is_err(), Ordering::SeqCst);
        complete_native(
            || draft::native_done(&mut state.write(), &binding, &mounted.mount, result.is_ok()),
            finish_reply,
            result.is_ok(),
        );
    });
}

// Caller rotation may run immediately when the oneshot becomes ready. Clear
// the exact retained native slot first; a JS release message alone is not done.
fn complete_native(
    cleanup: impl FnOnce(),
    reply: Option<(oneshot::Sender<Result<Value, Error>>, Value)>,
    success: bool,
) {
    cleanup();
    if let Some((reply, value)) = reply {
        let _ = reply.send(if success {
            Ok(value)
        } else {
            Err(unavailable())
        });
    }
}

async fn request(mounted: &Mounted, payload: Value) -> Result<Value, Error> {
    if mounted.failed() {
        return Err(unavailable());
    }
    let (reply, receive) = oneshot::channel();
    mounted
        .sender
        .send(Command {
            payload,
            reply: Some(reply),
        })
        .map_err(|_| unavailable())?;
    tokio::time::timeout(TIMEOUT, receive)
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
}

/// A short native lease precedes all controller mutation. Awaiting controller
/// I/O may retain it; only the original editor wrapper is inert, not a new view.
pub(crate) struct Lease {
    mounted: Mounted,
    request: String,
    sequence: u64,
    blocks: Vec<Value>,
    released: bool,
}
impl Lease {
    pub(crate) fn blocks(&self) -> &[Value] {
        &self.blocks
    }
    async fn end(mut self, finish: bool) -> Result<(), Error> {
        let result=request(&self.mounted,json!({"kind":if finish{"finish"}else{"release"},"lease":self.request,"sequence":self.sequence})).await?;
        self.released = true;
        if result["kind"] != "release-result"
            || result["ok"] != true
            || result["finished"] != finish
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) async fn release(self) -> Result<(), Error> {
        self.end(false).await
    }
    pub(crate) async fn finish(self) -> Result<(), Error> {
        self.end(true).await
    }
    /// Recheck the original sealed node after persistence without finishing or
    /// unlocking it. Failure leaves normal RAII release/recovery responsible.
    pub(crate) async fn validate(&self) -> Result<(), Error> {
        let result = request(
            &self.mounted,
            json!({"kind":"validate", "lease":self.request,
            "sequence":self.sequence}),
        )
        .await?;
        if result["kind"] != "release-result"
            || result["validated"] != true
            || result["ok"] != true
            || result["finished"] != false
        {
            return Err(unavailable());
        }
        Ok(())
    }
    /// The caller already replaced the original view synchronously while this
    /// lease was held. Keep its exact wrapper inert until the retained bridge
    /// observes detachment and acknowledges its final snapshot. Unlike finish,
    /// this cannot consume a still-visible editor before route validation.
    pub(crate) fn departed(mut self) {
        self.released = true;
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.mounted.sender.send(Command {
                payload: json!({"kind":"release","lease":self.request,"sequence":self.sequence}),
                reply: None,
            });
        }
    }
}

pub(crate) async fn capture(state: Signal<ShellState>, binding: &Binding) -> Result<Lease, Error> {
    let mounted = {
        let shell = state.peek();
        if !binding.visible_mount(&shell) {
            return Err(unavailable());
        }
        draft::native(&shell, binding)
            .filter(|m| m.matches(binding))
            .ok_or_else(unavailable)?
    };
    let value = request(&mounted, json!({"kind":"capture"})).await?;
    if value["kind"] != "capture"
        || value["locked"] != true
        || value["detached"] != false
        || value["composing"] != false
    {
        return Err(unavailable());
    }
    let lease = Lease {
        mounted,
        request: value["request"]
            .as_str()
            .ok_or_else(unavailable)?
            .to_owned(),
        sequence: value["sequence"].as_u64().ok_or_else(unavailable)?,
        blocks: serde_json::from_value(value["blocks"].clone()).map_err(|_| unavailable())?,
        released: false,
    };
    if !binding.visible_mount(&state.peek()) {
        return Err(unavailable());
    }
    Ok(lease)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refused_route_releases_lease_but_committed_route_keeps_seal_until_detach() {
        for committed in [false, true] {
            let (sender, mut receiver) = mpsc::unbounded_channel();
            let lease = Lease {
                mounted: Mounted {
                    mount: "original-mount".into(),
                    scope: "original-scope".into(),
                    generation: 7,
                    sequence: Arc::new(AtomicU64::new(4)),
                    failed: Arc::new(AtomicBool::new(false)),
                    sender,
                },
                request: "original-request".into(),
                sequence: 4,
                blocks: vec![json!({"type":"rich_text","opaque":"retained"})],
                released: false,
            };
            if committed {
                lease.departed();
            } else {
                drop(lease);
            }
            if committed {
                assert!(receiver.try_recv().is_err());
            } else {
                let command = receiver.try_recv().unwrap();
                assert_eq!(
                    command.payload,
                    json!({"kind":"release","lease":"original-request","sequence":4})
                );
                assert!(command.reply.is_none());
                assert!(receiver.try_recv().is_err());
            }
        }
    }
    #[test]
    fn finish_receipt_cannot_unblock_rotation_before_native_cleanup() {
        for success in [true, false] {
            let (sender, mut receiver) = oneshot::channel();
            let cleaned = AtomicBool::new(false);
            let value = json!({"kind":"release-result","ok":true,"finished":true});
            complete_native(
                || {
                    assert!(matches!(
                        receiver.try_recv(),
                        Err(oneshot::error::TryRecvError::Empty)
                    ));
                    cleaned.store(true, Ordering::SeqCst);
                },
                Some((sender, value.clone())),
                success,
            );
            assert!(cleaned.load(Ordering::SeqCst));
            let result = receiver.try_recv().unwrap();
            if success {
                assert_eq!(result.unwrap(), value);
            } else {
                assert!(result.is_err());
            }
        }
    }
}

/// Constructor for host-custody tests; never installs or asserts renderer state.
#[cfg(test)]
pub(crate) fn test_mounted(binding: &Binding, mount: &str) -> Mounted {
    let (sender, _) = mpsc::unbounded_channel();
    Mounted {
        mount: mount.into(),
        scope: binding.scope(),
        generation: binding.generation(),
        sequence: Arc::new(AtomicU64::new(0)),
        failed: Arc::new(AtomicBool::new(false)),
        sender,
    }
}
