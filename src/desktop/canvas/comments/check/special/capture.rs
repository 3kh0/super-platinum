use super::*;

pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    target: Option<&str>,
    reads: usize,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    wait(
        desktop,
        "document.activeElement?.id==='canvas-comments-back'",
    )
    .await?;
    let script = include_str!("body.js").replace("__TARGET__", &json!(target).to_string());
    let scroll = include_str!("scroll.js").replace("__TARGET__", &json!(target).to_string());
    eval(desktop, &scroll).await?;
    let mut layout = eval(desktop, include_str!("../../layout.js")).await?;
    let bodies = eval(desktop, &script).await?;
    layout["bodyProof"] = bodies.clone();
    let proof = data::proof(&state.peek());
    if proof.is_null()
        || proof["retained"]["ownerCurrent"] != true
        || proof["retained"]["loading"] != false
        || !proof["retained"]["rows"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|row| row["matchesOriginal"] == true))
        || proof["requests"].as_array().map(Vec::len) != Some(reads)
        || proof["responses"].as_array().map(Vec::len) != Some(reads)
        || !proof["media"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["registered"] == 0))
    {
        return Err(format!("{name}: original read/media proof differs"));
    }
    if let Some(hidden) = bodies["rows"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["ts"] == data::HIDDEN))
    {
        if hidden["text"] != "This content was deleted."
            || hidden["links"] != json!([])
            || hidden["images"] != json!([])
            || hidden["attachments"] != json!([])
        {
            return Err("Hidden root leaked body/media".into());
        }
    }
    let ordinal = stages.len();
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"layout":layout,"proof":proof});
    let emitted = receipt.clone();
    tokio::task::spawn_blocking(move || eprintln!("canvas-special-roots-stage: {emitted}"))
        .await
        .map_err(|_| "Stage logging failed")?;
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Private special-root ACK directory missing")?;
    let ack = directory.join(format!("{ordinal}.ack"));
    for _ in 0..600 {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK worker failed")?
        {
            Ok(v) if v == b"captured\n" => {
                if data::proof(&state.peek()) != proof || eval(desktop, &script).await? != bodies {
                    return Err(format!(
                        "{name}: original body/source changed during capture"
                    ));
                }
                stages.push(receipt);
                return Ok(());
            }
            Ok(_) => return Err("Malformed special-root ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Cannot read special-root ACK".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("{name}: capture ACK timed out"))
}
