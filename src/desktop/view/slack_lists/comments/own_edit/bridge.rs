//! Retained exact-node capture. DOM IDs locate a mount once; they never authorize
//! a later edit. The edit controller retains each capture in memory before ACKing it.
//! The independent reply journal is never read or written by this adapter.
use crate::{
    slack_lists::comments::own_edit::{self as controller, Binding, content::Content},
    state::ShellState,
};
use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use super_platinum_core::slack::Error;
use tokio::sync::{mpsc, oneshot};

const SCRIPT: &str = include_str!("../bridge.js");
mod events;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);
fn unavailable() -> Error {
    Error::Api("list_comment_edit_native_capture_unavailable".into())
}

#[derive(Clone)]
pub(crate) struct Mounted {
    mount: String,
    scope: String,
    generation: u64,
    representation: &'static str,
    sequence: Arc<AtomicU64>,
    stamp: Arc<Mutex<Option<NativeStamp>>>,
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
#[derive(Clone, Copy)]
struct NativeStamp {
    sequence: u64,
    revision: u64,
    pending: bool,
    composing: bool,
    detached: bool,
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
    revision: u64,
    composing: bool,
    generation: u64,
    detached: bool,
    #[serde(rename = "finalCapture")]
    final_capture: bool,
}

pub(crate) fn mount(mut state: Signal<ShellState>, binding: Binding) {
    let Some(representation) = binding.representation(&state.peek()) else {
        return;
    };
    let (sender, commands) = mpsc::unbounded_channel();
    let mounted = Mounted {
        mount: uuid::Uuid::new_v4().to_string(),
        scope: binding.scope(),
        generation: binding.generation(),
        representation,
        sequence: Arc::new(AtomicU64::new(0)),
        stamp: Arc::new(Mutex::new(None)),
        failed: Arc::new(AtomicBool::new(false)),
        sender,
    };
    let installed = {
        let mut shell = state.write();
        binding.visible_mount(&shell)
            && binding.representation(&shell) == Some(mounted.representation)
            && controller::install_native(&mut shell, &binding, mounted.clone())
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
        let Some(old) = controller::native(&shell, &binding).filter(|m| {
            m.matches(&binding)
                && m.failed()
                && binding.representation(&shell) == Some(m.representation)
        }) else {
            return false;
        };
        Mounted {
            sender,
            failed: Arc::new(AtomicBool::new(false)),
            ..old
        }
    };
    if !controller::install_native(&mut state.write(), &binding, mounted.clone()) {
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
        let options = json!({"id":"slack-list-comment-edit-draft","scope":mounted.scope,
            "generation":mounted.generation,"mount":mounted.mount,"recover":recover,
            "representation":mounted.representation});
        let clipboard_script = crate::slack_lists::comments::delta_editor::CLIPBOARD_BRIDGE;
        let mut eval = dioxus::document::eval(&format!(
            "{clipboard_script}\n{SCRIPT}\nreturn await listCommentsComposeBridge({options},dioxus);"
        ));
        let mut completion = Box::pin(eval.join::<Value>());
        let mut pending: Option<(String, Command)> = None;
        let mut final_sequence = None;
        let mut finish_reply = None;
        let mut completed = false;
        let mut completion_finished = false;
        let mut clipboard = events::Clipboard::new();
        let result:Result<(),Error>=async {
            loop {
                // Eval completion and the last message can become ready in either
                // order. A successful finish requires both before native_done.
                if completed && pending.as_ref().is_none_or(|(_,c)|c.payload["kind"]!="finish") { return Ok(()); }
                tokio::select! {
                    ready=clipboard.next() => {
                        if let Some(value)=clipboard.finish(state,&binding,&mounted,ready)? {
                            eval.send(value).map_err(|_|unavailable())?;
                        }
                    }
                    done=&mut completion, if !completed => {
                        completion_finished=true;
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
                                let (content,pending_content)=Content::decode_capture(&value,mounted.representation)?;
                                if binding.representation(&state.peek())!=Some(mounted.representation) { return Err(unavailable()); }
                                controller::retain_content(&mut state.write(),&binding,&mounted.mount,capture.sequence,
                                    content,capture.composing,pending_content,capture.detached)?;
                                controller::persist_capture(state,&binding).await?;
                                mounted.sequence.store(capture.sequence,Ordering::SeqCst);
                                eval.send(json!({"kind":"ack","scope":mounted.scope,"mount":mounted.mount,"sequence":capture.sequence})).map_err(|_|unavailable())?;
                                *mounted.stamp.lock().map_err(|_|unavailable())?=Some(NativeStamp {
                                    sequence:capture.sequence,revision:capture.revision,pending:pending_content,
                                    composing:capture.composing,detached:capture.detached,
                                });
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
                            Some("editor-action") => { events::action(state,&binding,&mounted,&value)?; }
                            Some("clipboard-request") => {
                                if let Some(rejection)=clipboard.start(state,&binding,&mounted,value)? {
                                    eval.send(rejection).map_err(|_|unavailable())?;
                                }
                            }
                            Some("clipboard-cancel") => { clipboard.cancel(&value); }
                            Some("clipboard-result") => {
                                // A renderer insertion result is neither durable capture nor send authority.
                                if mounted.representation!="plain-delta" || !value["applied"].is_boolean() {
                                    return Err(unavailable());
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
        clipboard.stop();
        if result.is_err() && !completion_finished {
            // A rejected capture has no durable ACK. Abort the original receive
            // loop so its finally block unseals the exact node before recovery.
            // A transport/renderer failure can still time out; it stays failed.
            let _ = eval
                .send(json!({"kind":"capture-abort","scope":mounted.scope,"mount":mounted.mount}));
            let _ = tokio::time::timeout(TIMEOUT, &mut completion).await;
        }
        if let Some((_, command)) = pending {
            if let Some(reply) = command.reply {
                let _ = reply.send(Err(unavailable()));
            }
        }
        mounted.failed.store(result.is_err(), Ordering::SeqCst);
        complete_native(
            || {
                controller::native_done(
                    &mut state.write(),
                    &binding,
                    &mounted.mount,
                    result.is_ok(),
                )
            },
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
struct NativeLock {
    mounted: Mounted,
    request: String,
    sequence: u64,
    released: bool,
}
impl Drop for NativeLock {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.mounted.sender.send(Command {
                payload: json!({"kind":"release","lease":self.request,"sequence":self.sequence}),
                reply: None,
            });
        }
    }
}

/// Only minted from a locked, admitted native capture. The original opaque
/// ticket is still independently validated/consumed by Lease.
#[derive(Clone)]
pub(crate) struct NoAttachmentEffects {
    mounted: Mounted,
    sequence: u64,
}
impl NoAttachmentEffects {
    pub(crate) fn current(&self, shell: &ShellState, binding: &Binding) -> bool {
        binding.current(shell)
            && !self.mounted.failed()
            && self.mounted.sequence.load(Ordering::SeqCst) == self.sequence
            && controller::native(shell, binding).is_some_and(|m| {
                m.matches(binding)
                    && m.mount == self.mounted.mount
                    && Arc::ptr_eq(&m.sequence, &self.mounted.sequence)
                    && !m.failed()
            })
    }
}

pub(crate) struct Lease {
    no_attachment_effects: bool,
    lock: NativeLock,
    content: Content,
}
impl Lease {
    /// A fresh successful capture of this installed plain editor, never a
    /// deserialized draft or an inference from original message attachments.
    pub(crate) fn no_attachment_effects(&self) -> Option<NoAttachmentEffects> {
        self.no_attachment_effects.then(|| NoAttachmentEffects {
            mounted: self.lock.mounted.clone(),
            sequence: self.lock.sequence,
        })
    }
    pub(crate) fn content(&self) -> &Content {
        &self.content
    }
    pub(crate) fn blocks(&self) -> Result<&[Value], Error> {
        self.content.rich_blocks().ok_or_else(unavailable)
    }
    async fn end(mut self, finish: bool) -> Result<(), Error> {
        let result=request(&self.lock.mounted,json!({"kind":if finish{"finish"}else{"release"},"lease":self.lock.request,"sequence":self.lock.sequence})).await?;
        if result["kind"] != "release-result"
            || result["ok"] != true
            || result["finished"] != finish
        {
            return Err(unavailable());
        }
        self.lock.released = true;
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
            &self.lock.mounted,
            json!({"kind":"validate", "lease":self.lock.request,
            "sequence":self.lock.sequence}),
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
}

fn captured_lease(mounted: Mounted, value: &Value) -> Result<Lease, Error> {
    // Metadata is admitted by the retained receive loop before delivery. Never
    // send a release naming a foreign node; arm cleanup before content decoding.
    if value["scope"] != mounted.scope
        || value["mount"] != mounted.mount
        || value["generation"].as_u64() != Some(mounted.generation)
        || value["locked"] != true
    {
        return Err(unavailable());
    }
    let request = value["request"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(unavailable)?
        .to_owned();
    let sequence = value["sequence"]
        .as_u64()
        .filter(|s| *s > 0 && *s <= 9_007_199_254_740_991)
        .ok_or_else(unavailable)?;
    let lock = NativeLock {
        mounted,
        request,
        sequence,
        released: false,
    };
    if value["kind"] != "capture" || value["detached"] != false || value["composing"] != false {
        return Err(unavailable());
    }
    let (content, pending) = Content::decode_capture(value, lock.mounted.representation)?;
    if pending {
        return Err(unavailable());
    }
    let no_attachment_effects =
        lock.mounted.representation == "plain-delta" && value["attachmentEffects"] == "none-v1";
    Ok(Lease {
        lock,
        content,
        no_attachment_effects,
    })
}

pub(crate) async fn capture(state: Signal<ShellState>, binding: &Binding) -> Result<Lease, Error> {
    let mounted = {
        let shell = state.peek();
        if !binding.visible_mount(&shell) {
            return Err(unavailable());
        }
        controller::native(&shell, binding)
            .filter(|m| {
                m.matches(binding) && binding.representation(&shell) == Some(m.representation)
            })
            .ok_or_else(unavailable)?
    };
    let value = request(&mounted, json!({"kind":"capture"})).await?;
    let lease = captured_lease(mounted, &value)?;
    let shell = state.peek();
    if !binding.visible_mount(&shell)
        || binding.representation(&shell) != Some(lease.content.representation())
    {
        return Err(unavailable());
    }
    Ok(lease)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture_mount(
        representation: &'static str,
    ) -> (Mounted, mpsc::UnboundedReceiver<Command>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Mounted {
                mount: "original-edit-mount".into(),
                scope: "original-edit-scope".into(),
                generation: 7,
                representation,
                sequence: Arc::new(AtomicU64::new(4)),
                stamp: Arc::new(Mutex::new(None)),
                failed: Arc::new(AtomicBool::new(false)),
                sender,
            },
            receiver,
        )
    }
    fn fixture_capture() -> Value {
        json!({"kind":"capture","scope":"original-edit-scope","mount":"original-edit-mount",
            "generation":7,"sequence":4,"request":"original-edit-request","locked":true,
            "detached":false,"composing":false,"blocks":[]})
    }
    #[test]
    fn attachment_effects_requires_fresh_locked_plain_marker_not_content_or_rich_capture() {
        for marker in [
            None,
            Some(json!(null)),
            Some(json!("future")),
            Some(json!("none-v1")),
        ] {
            let (mounted, _receiver) = fixture_mount("plain-delta");
            let mut raw = fixture_capture();
            raw.as_object_mut().unwrap().remove("blocks");
            raw["delta"] = json!({"mode":"plain-text","ops":[{"insert":"original\n"}]});
            raw["pending"] = json!(false);
            if let Some(marker) = &marker {
                raw["attachmentEffects"] = marker.clone();
            }
            let lease = captured_lease(mounted, &raw).unwrap();
            assert_eq!(
                lease.no_attachment_effects().is_some(),
                marker == Some(json!("none-v1"))
            );
        }
        let (mounted, _receiver) = fixture_mount("rich-blocks");
        let mut raw = fixture_capture();
        raw["attachmentEffects"] = json!("none-v1");
        assert!(
            captured_lease(mounted, &raw)
                .unwrap()
                .no_attachment_effects()
                .is_none()
        );
        let (mounted, _receiver) = fixture_mount("plain-delta");
        raw.as_object_mut().unwrap().remove("blocks");
        raw["delta"] = json!({"mode":"plain-text","ops":[]});
        raw["pending"] = json!(false);
        raw["locked"] = json!(false);
        assert!(captured_lease(mounted, &raw).is_err());
    }
    #[test]
    fn malformed_locked_content_and_pending_capture_release_original_lease() {
        for payload in [
            json!({"delta":{"mode":"plain-text","ops":[{"delete":1}]},"pending":false}),
            json!({"delta":{"mode":"plain-text","ops":[{"insert":"body\n"}]},"pending":true}),
            json!({"delta":{"mode":"plain-text","ops":[]}}),
        ] {
            let (mounted, mut receiver) = fixture_mount("plain-delta");
            let mut raw = fixture_capture();
            raw.as_object_mut().unwrap().remove("blocks");
            raw.as_object_mut()
                .unwrap()
                .extend(payload.as_object().unwrap().clone());
            assert!(captured_lease(mounted, &raw).is_err());
            assert_eq!(
                receiver.try_recv().unwrap().payload,
                json!({"kind":"release","lease":"original-edit-request","sequence":4})
            );
            assert!(receiver.try_recv().is_err());
        }
    }
    #[test]
    fn foreign_capture_cannot_release_or_adopt_replacement() {
        for key in ["scope", "mount", "generation"] {
            let (mounted, mut receiver) = fixture_mount("rich-blocks");
            let mut raw = fixture_capture();
            raw[key] = json!("replacement");
            assert!(captured_lease(mounted, &raw).is_err());
            assert!(receiver.try_recv().is_err());
        }
    }
    #[test]
    fn plain_lease_cannot_be_read_as_rich_blocks() {
        let (mounted, _receiver) = fixture_mount("plain-delta");
        let mut raw = fixture_capture();
        raw.as_object_mut().unwrap().remove("blocks");
        raw["delta"] = json!({"mode":"plain-text","ops":[{"insert":"body\n"}]});
        raw["pending"] = json!(false);
        let lease = captured_lease(mounted, &raw).unwrap();
        assert!(lease.blocks().is_err());
        assert_eq!(
            lease.content().plain_delta().unwrap().ops(),
            &[json!({"insert":"body\n"})]
        );
    }
    #[tokio::test]
    async fn malformed_finish_response_keeps_raii_release_armed() {
        let (mounted, mut receiver) = fixture_mount("rich-blocks");
        let lease = captured_lease(mounted, &fixture_capture()).unwrap();
        let responder = async {
            let command = receiver.recv().await.unwrap();
            assert_eq!(command.payload["kind"], "finish");
            command
                .reply
                .unwrap()
                .send(Ok(
                    json!({"kind":"release-result","ok":true,"finished":false}),
                ))
                .unwrap();
        };
        let (result, ()) = tokio::join!(lease.finish(), responder);
        assert!(result.is_err());
        assert_eq!(
            receiver.try_recv().unwrap().payload,
            json!({"kind":"release","lease":"original-edit-request","sequence":4})
        );
    }
    #[test]
    fn abandoned_edit_capture_releases_only_its_original_native_lease() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let lease = Lease {
            no_attachment_effects: false,
            lock: NativeLock {
                mounted: Mounted {
                    mount: "original-edit-mount".into(),
                    scope: "original-edit-scope".into(),
                    generation: 7,
                    representation: "rich-blocks",
                    sequence: Arc::new(AtomicU64::new(4)),
                    stamp: Arc::new(Mutex::new(None)),
                    failed: Arc::new(AtomicBool::new(false)),
                    sender,
                },
                request: "original-edit-request".into(),
                sequence: 4,
                released: false,
            },
            content: Content::RichBlocks(vec![
                json!({"type":"rich_text","opaque":"retained edit"}),
            ]),
        };
        drop(lease);
        let command = receiver.try_recv().unwrap();
        assert_eq!(
            command.payload,
            json!({"kind":"release","lease":"original-edit-request","sequence":4})
        );
        assert!(command.reply.is_none());
        assert!(receiver.try_recv().is_err());
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
        representation: "rich-blocks",
        sequence: Arc::new(AtomicU64::new(0)),
        stamp: Arc::new(Mutex::new(None)),
        failed: Arc::new(AtomicBool::new(false)),
        sender,
    }
}

#[cfg(test)]
pub(crate) fn test_failed_mounted(binding: &Binding, mount: &str) -> Mounted {
    let mounted = test_mounted(binding, mount);
    mounted.failed.store(true, Ordering::SeqCst);
    mounted
}
