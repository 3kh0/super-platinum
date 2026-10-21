use super::*;

pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    id: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !data::enabled(&state.peek()) {
        return Err("Heading annotation lost original owner".into());
    }
    let ordinal = stages.len();
    let before = data::proof(&state.peek());
    let local = journal(state).await?;
    let runtime = model(&state.peek());
    let geometry_script = include_str!("geometry.js").replace("__ID__", &json!(id).to_string());
    let scroll_script = include_str!("scroll.js").replace("__ID__", &json!(id).to_string());
    checked(desktop, &scroll_script).await?;
    let geometry = wait(desktop, &geometry_script)
        .await
        .map_err(|e| format!("{name} capture geometry: {e}"))?;
    if data::proof(&state.peek()) != before || model(&state.peek()) != runtime {
        return Err("Annotation changed during geometry".into());
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"fixture":before,"model":runtime,"journal":local,"geometry":geometry});
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Missing headings capture ACK directory")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Heading capture directory must be absolute".into());
    }
    let emitted = receipt.clone();
    tokio::task::spawn_blocking(move || eprintln!("{}-stage: {emitted}", data::receipt_prefix()))
        .await
        .map_err(|_| "Heading receipt worker failed")?;
    let ack = directory.join(format!("{ordinal}.ack"));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Heading capture ACK worker failed")?
        {
            Ok(v) if v == b"captured\n" => break,
            Ok(_) => return Err("Malformed headings capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Heading capture ACK read failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Heading capture timed out: {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let after = checked(desktop, &geometry_script).await?;
    if geometry != after
        || data::proof(&state.peek()) != before
        || model(&state.peek()) != runtime
        || journal(state).await? != local
    {
        return Err(format!("Heading changed during {name} screenshot"));
    }
    stages.push(receipt);
    Ok(())
}
