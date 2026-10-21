use super::*;

pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    ordinal: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !fixture::enabled(&state.peek()) {
        return Err("Heading stage lost disconnected owner".into());
    }
    let before = fixture::proof(&state.peek());
    if fixture::has_content_id() {
        fixture::validate_attributes(&before)?;
    }
    let editor = checked(
        desktop,
        r#"({ok:true,present:!!document.getElementById('canvas-text-editor')})"#,
    )
    .await?["present"]
        == true;
    let editor_layout = if editor {
        super::editor_layout(desktop).await?
    } else {
        Value::Null
    };
    let geometry = wait(desktop, include_str!("geometry.js")).await?;
    if fixture::proof(&state.peek()) != before {
        return Err("Heading source changed during geometry".into());
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"fixture":before,"geometry":geometry,"editorLayout":editor_layout});
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Missing heading capture ACK directory")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Heading capture directory must be absolute".into());
    }
    eprintln!("{}-stage: {receipt}", fixture::prefix());
    let ack = directory.join(format!("{ordinal}.ack"));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Heading capture ACK worker failed")?
        {
            Ok(v) if v == b"captured\n" => break,
            Ok(_) => return Err("Malformed heading capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Heading capture ACK read failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Heading capture timed out: {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let after = checked(desktop, include_str!("geometry.js")).await?;
    if geometry != after || fixture::proof(&state.peek()) != before {
        return Err(format!("Heading changed during {name} screenshot"));
    }
    stages.push(receipt);
    Ok(())
}
