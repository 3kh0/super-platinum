use super::*;
fn alias(name: &str) -> &str {
    match name {
        "reopened" => "profile",
        other => other,
    }
}
fn script(name: &str) -> String {
    include_str!("../state.js").replace("__STAGE__", &json!(alias(name)).to_string())
}
pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    wait(desktop,"[...document.querySelectorAll('.canvas-attribution img,#canvas-comment-profile img,.profile-hover img')].every(e=>e.complete&&e.naturalWidth>0)").await?;
    eval(
        desktop,
        &include_str!("../scroll.js").replace("__STAGE__", &json!(alias(name)).to_string()),
    )
    .await?;
    let script = script(name);
    let layout = eval(desktop, &script).await?;
    let proof = proof(&state.peek());
    let reply = reply::fixture::attribution_snapshot(state).await?;
    if !recovery_data::enabled(&state.peek())
        || proof["retained"]["ownerCurrent"] != true
        || proof["retained"]["loading"] != false
    {
        return Err(format!("{name}: original Canvas owner/read retired"));
    }
    let ordinal = stages.len();
    if (1..=11).contains(&ordinal) && layout["native"]["sameOriginal"] != true {
        return Err(format!(
            "{name}: original rich reply node or blocks changed"
        ));
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":false,"noTransport":false,
        "isolation":data::isolation(&state.peek()),"layout":layout,"proof":proof,"reply":reply});
    let emitted = receipt.clone();
    tokio::task::spawn_blocking(move || eprintln!("canvas-editor-recovery-stage: {emitted}"))
        .await
        .map_err(|_| "Stage logging failed")?;
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Original ACK directory missing")?;
    let ack = directory.join(format!("{ordinal}.ack"));
    for _ in 0..600 {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "ACK worker failed")?
        {
            Ok(bytes) if bytes == b"captured\n" => {
                if eval(desktop, &script).await? != layout
                    || super::proof(&state.peek()) != proof
                    || reply::fixture::attribution_snapshot(state).await? != reply
                {
                    return Err(format!(
                        "{name}: original native/source/disk changed during capture"
                    ));
                }
                stages.push(receipt);
                return Ok(());
            }
            Ok(_) => return Err("Malformed recovery capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Recovery ACK read failed".into()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(format!("{name}: original capture ACK timed out"))
}
