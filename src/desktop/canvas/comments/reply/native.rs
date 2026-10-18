//! One root-owned evaluation retains the original rich node through unmount.
//! Only cloned snapshots cross JSON. The editor's replacement ticket stays JS-owned.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

const BRIDGE: &str = include_str!("native.js");
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

#[derive(Clone)]
pub(super) struct Mounted {
    mount: String,
    scope: String,
    generation: u64,
    sequence: u64,
    failed: bool,
    sender: mpsc::Sender<Command>,
}
impl Mounted {
    pub(super) fn matches(&self, binding: &Binding) -> bool {
        self.scope == binding.field_scope() && self.generation == binding.read
    }
}
struct Command {
    payload: Value,
    reply: oneshot::Sender<Result<Value, ()>>,
}
#[derive(Deserialize)]
struct Capture {
    kind: String,
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

/// Mount only after durable local inventory has loaded. The task is retained by
/// the application root; retiring a visible Binding does not drop captured input.
pub(super) fn mount(mut state: Signal<ShellState>, binding: Binding) {
    let (sender, mut commands) = mpsc::channel::<Command>(4);
    let mounted = Mounted {
        mount: uuid::Uuid::new_v4().to_string(),
        scope: binding.field_scope(),
        generation: binding.read,
        sequence: 0,
        failed: false,
        sender,
    };
    {
        let mut shell = state.write();
        if !binding.owner.current(&shell) {
            return;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.native.is_some() || e.loading || e.inventory.is_none() {
            return;
        }
        e.native = Some(mounted.clone());
    }
    launch(state, binding, mounted, commands, false);
}

/// A failed bridge can resume only its privately retained original JS node.
/// It cannot adopt a replacement with the same DOM id or discard an old tail.
pub(super) fn retry(mut state: Signal<ShellState>, binding: Binding) -> bool {
    let (sender, commands) = mpsc::channel::<Command>(4);
    let mounted = {
        let mut shell = state.write();
        if !binding.owner.current(&shell) {
            return false;
        }
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return false;
        };
        let Some(mounted) = e.native.as_mut().filter(|n| n.failed) else {
            return false;
        };
        mounted.failed = false;
        mounted.sender = sender;
        mounted.clone()
    };
    launch(state, binding, mounted, commands, true);
    true
}

fn launch(
    mut state: Signal<ShellState>,
    binding: Binding,
    mounted: Mounted,
    mut commands: mpsc::Receiver<Command>,
    recover: bool,
) {
    dioxus::core::spawn_forever(async move {
        let options = json!({"id":"canvas-comment-reply", "scope":mounted.scope,
            "mount":mounted.mount, "generation":mounted.generation,"recover":recover});
        let script = format!("{BRIDGE}\nreturn await canvasRichReplyBridge({options},dioxus);");
        let mut eval = dioxus::document::eval(&script);
        let mut completion = Box::pin(eval.join::<Value>());
        let mut pending: Option<(String, oneshot::Sender<Result<Value, ()>>)> = None;
        let mut detached = false;
        let mut last_sequence = mounted.sequence;
        let result: Result<(), ()> = async {
            loop {
                tokio::select! {
                    done = &mut completion => {
                        let value=done.map_err(|_| ())?;
                        if !detached || value["scope"]!=mounted.scope || value["mount"]!=mounted.mount
                            || value["final"]!=true || value["sequence"].as_u64()!=Some(last_sequence) { return Err(()); }
                        return Ok(());
                    }
                    command = commands.recv(), if pending.is_none() => {
                        let Some(command)=command else { return Err(()) };
                        let request=uuid::Uuid::new_v4().to_string();
                        let mut payload=command.payload;
                        payload["scope"]=json!(mounted.scope);
                        payload["mount"]=json!(mounted.mount);
                        payload["request"]=json!(request);
                        eval.send(payload).map_err(|_| ())?;
                        pending=Some((request,command.reply));
                    }
                    message = eval.recv::<Value>() => {
                        let value=message.map_err(|_| ())?;
                        if value["scope"]!=mounted.scope || value["mount"]!=mounted.mount { return Err(()); }
                        match value["kind"].as_str() {
                            Some("capture") => {
                                let capture:Capture=serde_json::from_value(value.clone()).map_err(|_| ())?;
                                retain_capture(state,&binding.id,&mounted,&capture)?;
                                last_sequence=capture.sequence;
                                // Root custody is established before acknowledgment. Disk failure
                                // keeps this exact content and entry charged for explicit recovery.
                                eval.send(json!({"kind":"ack","scope":mounted.scope,"mount":mounted.mount,
                                    "sequence":capture.sequence})).map_err(|_| ())?;
                                save(state,binding.id.clone());
                                detached=capture.detached || capture.final_capture;
                                if let Some(request)=capture.request.as_deref() {
                                    if pending.as_ref().map(|p|p.0.as_str())!=Some(request){return Err(());}
                                    let (_,reply)=pending.take().unwrap();
                                    let _=reply.send(Ok(value));
                                }
                            }
                            Some("replace-result" | "finish-refused") => {
                                let request=value["request"].as_str().ok_or(())?;
                                if pending.as_ref().map(|p|p.0.as_str())!=Some(request){return Err(());}
                                let (_,reply)=pending.take().unwrap();
                                let _=reply.send(Ok(value));
                            }
                            _=>return Err(()),
                        }
                    }
                }
            }
        }.await;
        if let Some((_, reply)) = pending {
            let _ = reply.send(Err(()));
        }
        let mut shell = state.write();
        let Some(e) = entry_mut(&mut shell, &binding.id) else {
            return;
        };
        if e.native.as_ref().is_some_and(|n| n.mount == mounted.mount) {
            if result.is_ok() {
                e.native = None;
            } else {
                e.native.as_mut().unwrap().failed = true;
                // Keep the failed native slot charged: no claim that final input
                // was captured, and no eviction based only on the last disk row.
                e.error = Some(
                    "The rich reply capture stopped. The captured draft is retained; keep this window open and review it before leaving.",
                );
            }
        }
    });
}

fn retain_capture(
    mut state: Signal<ShellState>,
    id: &str,
    mounted: &Mounted,
    value: &Capture,
) -> Result<(), ()> {
    retain_value(&mut state.write(), id, mounted, value)
}
fn retain_value(
    shell: &mut ShellState,
    id: &str,
    mounted: &Mounted,
    value: &Capture,
) -> Result<(), ()> {
    let e = entry_mut(shell, id).ok_or(())?;
    let current = e.native.as_mut().ok_or(())?;
    if value.kind != "capture"
        || value.mount != current.mount
        || value.mount != mounted.mount
        || value.scope != current.scope
        || value.generation != current.generation
        || value.sequence <= current.sequence
        || value.sequence > 9_007_199_254_740_991
        || value.revision > 9_007_199_254_740_991
    {
        return Err(());
    }
    current.sequence = value.sequence;
    e.composing = value.composing;
    if e.blocks.as_deref() != Some(value.blocks.as_slice()) {
        e.blocks = Some(value.blocks.clone());
        e.serial = e.serial.checked_add(1).ok_or(())?;
        e.capture = uuid::Uuid::new_v4().to_string();
        e.retired_text = None;
        e.retired_blocks = None;
        e.reviewed = false;
        match super_platinum_core::canvas::comments::content::fallback(&value.blocks) {
            Ok(text) => {
                e.text = text;
                e.error = None;
            }
            Err(_) => {
                // Unknown rich bytes remain retained, but never become a plain
                // fallback send or an apparently saved supported draft.
                e.error = Some(
                    "This rich reply contains unsupported content. Its original content is retained and has not been sent.",
                );
            }
        }
    }
    Ok(())
}

async fn request(mounted: Mounted, payload: Value) -> Result<Value, ()> {
    let (reply, result) = oneshot::channel();
    tokio::time::timeout(TIMEOUT, async move {
        mounted
            .sender
            .send(Command { payload, reply })
            .await
            .map_err(|_| ())?;
        result.await.map_err(|_| ())?
    })
    .await
    .map_err(|_| ())?
}

/// A fresh actual-node capture, used before durable admission. Detached or
/// composing snapshots remain retained but cannot authorize a visible Send.
pub(super) async fn capture(
    state: Signal<ShellState>,
    binding: &Binding,
) -> Result<Vec<Value>, ()> {
    let mounted = entry(&state.peek(), &binding.id)
        .and_then(|e| e.native.clone())
        .ok_or(())?;
    if !mounted.matches(binding) {
        return Err(());
    }
    let result = request(mounted, json!({"kind":"capture"})).await?;
    if result["detached"] != false || result["composing"] != false {
        return Err(());
    }
    serde_json::from_value(result["blocks"].clone()).map_err(|_| ())
}

pub(super) async fn clear(
    state: Signal<ShellState>,
    binding: &Binding,
    serial: u64,
    expected: &[Value],
) -> Result<bool, ()> {
    let mounted = {
        let shell = state.peek();
        let e = entry(&shell, &binding.id).ok_or(())?;
        if e.serial != serial || e.composing || e.blocks.as_deref() != Some(expected) {
            return Ok(false);
        }
        e.native.clone().filter(|n| n.matches(binding)).ok_or(())?
    };
    let sequence = mounted.sequence;
    let result = request(
        mounted,
        json!({"kind":"replace","sequence":sequence,"blocks":[]}),
    )
    .await?;
    result["replaced"].as_bool().ok_or(())
}

pub(super) async fn finish(state: Signal<ShellState>, binding: &Binding) -> Result<(), ()> {
    let mounted = entry(&state.peek(), &binding.id)
        .and_then(|e| e.native.clone())
        .ok_or(())?;
    let result = request(mounted, json!({"kind":"finish"})).await?;
    if result["kind"] != "capture" || result["composing"] != false || result["finalCapture"] != true
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mounted(binding: &Binding) -> Mounted {
        let (sender, _receiver) = mpsc::channel(4);
        Mounted {
            mount: "original-node".into(),
            scope: binding.field_scope(),
            generation: binding.read,
            sequence: 0,
            failed: false,
            sender,
        }
    }
    fn capture(mounted: &Mounted, blocks: Vec<Value>) -> Capture {
        Capture {
            kind: "capture".into(),
            scope: mounted.scope.clone(),
            mount: mounted.mount.clone(),
            sequence: 1,
            request: None,
            blocks,
            revision: 2,
            composing: false,
            generation: mounted.generation,
            detached: true,
            final_capture: false,
        }
    }
    #[test]
    fn detached_original_rich_capture_survives_shell_replacement_without_foreign_send_authority() {
        let (mut previous, model, binding) = super::super::tests::setup();
        let mounted = mounted(&binding);
        entry_mut(&mut previous, &binding.id).unwrap().native = Some(mounted.clone());
        let mut loaded = ShellState::fixture(crate::media::MediaRegistry::default());
        loaded.core.active_account = Some(uuid::Uuid::new_v4().to_string());
        super::super::carry_pending(&mut previous, &mut loaded);
        let blocks = vec![
            json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"newer tail","style":{"bold":true}},{"type":"user","user_id":"U9"}]}]}),
        ];
        let receipt = capture(&mounted, blocks.clone());
        assert!(retain_value(&mut loaded, &binding.id, &mounted, &receipt).is_ok());
        let retained = entry(&loaded, &binding.id).unwrap();
        assert_eq!(retained.blocks.as_ref(), Some(&blocks));
        assert_eq!(retained.serial, 6);
        assert!(retained.sending);
        assert!(!super::super::delivery::current(
            &loaded, &model, &binding, 6
        ));
        assert!(!durably_idle(retained));
        assert!(previous.canvas_checklist.comment_replies.entries.is_empty());
    }
    #[test]
    fn stale_foreign_generation_and_duplicate_capture_do_not_replace_actual_owned_blocks() {
        let (mut shell, _, binding) = super::super::tests::setup();
        let mounted = mounted(&binding);
        entry_mut(&mut shell, &binding.id).unwrap().native = Some(mounted.clone());
        let original = vec![json!({"type":"opaque_future","nested":{"raw":"preserve"}})];
        let receipt = capture(&mounted, original.clone());
        assert!(retain_value(&mut shell, &binding.id, &mounted, &receipt).is_ok());
        assert!(entry(&shell, &binding.id).unwrap().error.is_some());
        for kind in 0..4 {
            let mut bad = capture(&mounted, vec![]);
            if kind == 0 {
                bad.mount = "replacement".into();
                bad.sequence = 2;
            }
            if kind == 1 {
                bad.generation += 1;
                bad.sequence = 2;
            }
            if kind == 2 {
                bad.scope = "foreign".into();
                bad.sequence = 2;
            }
            assert!(retain_value(&mut shell, &binding.id, &mounted, &bad).is_err());
            assert_eq!(
                entry(&shell, &binding.id).unwrap().blocks.as_ref(),
                Some(&original)
            );
        }
        assert_eq!(entry(&shell, &binding.id).unwrap().serial, 6);
    }
}
