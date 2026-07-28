use super::*;

static REPLIES: OnceLock<Mutex<HashMap<u64, oneshot::Sender<AgentResponse>>>> = OnceLock::new();
static ALLOW_DESTRUCTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static NEXT_FALLBACK_ID: AtomicU64 = AtomicU64::new(1);

fn replies() -> &'static Mutex<HashMap<u64, oneshot::Sender<AgentResponse>>> {
    REPLIES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(unix)]
pub fn enabled() -> bool {
    std::env::var_os("SNACK_AGENT").is_some()
}

#[cfg(not(unix))]
pub fn enabled() -> bool {
    false
}

pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("SNACK_AGENT_SOCK") {
        return PathBuf::from(path);
    }
    std::env::temp_dir().join("snack-agent.sock")
}

pub fn allow_destructive() -> bool {
    ALLOW_DESTRUCTIVE.load(Ordering::Relaxed)
        || std::env::var_os("SNACK_AGENT_ALLOW_DESTRUCTIVE").is_some()
}

pub fn set_allow_destructive(enabled: bool) {
    ALLOW_DESTRUCTIVE.store(enabled, Ordering::Relaxed);
}

#[cfg(unix)]
pub fn subscription() -> Subscription<Message> {
    Subscription::run(agent_stream)
}

#[cfg(not(unix))]
pub fn subscription() -> Subscription<Message> {
    Subscription::none()
}

#[cfg(unix)]
fn agent_stream() -> impl futures::Stream<Item = Message> {
    iced::stream::channel(64, |output| async move {
        if let Err(e) = serve(output).await {
            tracing::error!(error = %e, "agent control server stopped");
        }
    })
}

#[cfg(unix)]
async fn serve(output: iced_mpsc::Sender<Message>) -> Result<(), String> {
    let path = socket_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create sock dir: {e}"))?;
    }

    let listener =
        UnixListener::bind(&path).map_err(|e| format!("bind {}: {e}", path.display()))?;
    tracing::info!(path = %path.display(), "agent control listening (SNACK_AGENT)");

    let marker = std::env::temp_dir().join("snack-agent.sock.path");
    let _ = std::fs::write(&marker, path.to_string_lossy().as_bytes());

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("accept: {e}"))?;
        let output = output.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, output).await {
                tracing::debug!(error = %e, "agent client disconnected");
            }
        });
    }
}

#[cfg(unix)]
async fn handle_client(
    stream: UnixStream,
    mut output: iced_mpsc::Sender<Message>,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await.map_err(|e| format!("read: {e}"))? {
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<AgentRequest>(&line) {
            Ok(req) => dispatch_request(req, &mut output).await,
            Err(e) => AgentResponse {
                id: 0,
                ok: false,
                data: None,
                error: Some(format!("invalid request json: {e}")),
            },
        };

        let payload =
            serde_json::to_string(&response).map_err(|e| format!("encode response: {e}"))?;
        writer
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| format!("write: {e}"))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|e| format!("write: {e}"))?;
    }

    Ok(())
}

#[cfg(unix)]
async fn dispatch_request(
    req: AgentRequest,
    output: &mut iced_mpsc::Sender<Message>,
) -> AgentResponse {
    let id = if req.id == 0 {
        NEXT_FALLBACK_ID.fetch_add(1, Ordering::Relaxed)
    } else {
        req.id
    };

    match &req.cmd {
        AgentCommand::Ping => {
            return AgentResponse::ok(
                id,
                json!({
                    "pong": true,
                    "socket": socket_path().display().to_string(),
                    "allow_destructive": allow_destructive(),
                }),
            );
        }
        AgentCommand::Help => {
            return AgentResponse::ok(id, help_data());
        }
        _ => {}
    }

    if req.cmd.is_destructive() && !allow_destructive() {
        return AgentResponse::err(
            id,
            "destructive command blocked; set SNACK_AGENT_ALLOW_DESTRUCTIVE=1 or send allow-destructive",
        );
    }

    let (tx, rx) = oneshot::channel();
    {
        let mut map = replies().lock().expect("agent replies poisoned");
        map.insert(id, tx);
    }

    if output
        .send(Message::Runtime(crate::app::RuntimeMessage::AgentRequest {
            id,
            command: req.cmd,
        }))
        .await
        .is_err()
    {
        let _ = take_reply(id);
        return AgentResponse::err(id, "app runtime not accepting agent commands");
    }

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => AgentResponse::err(id, "reply channel closed"),
        Err(_) => {
            let _ = take_reply(id);
            AgentResponse::err(id, "timed out waiting for app to handle command")
        }
    }
}

fn take_reply(id: u64) -> Option<oneshot::Sender<AgentResponse>> {
    replies()
        .lock()
        .expect("agent replies poisoned")
        .remove(&id)
}

pub fn complete(id: u64, response: AgentResponse) {
    if let Some(tx) = take_reply(id) {
        let _ = tx.send(response);
    }
}
