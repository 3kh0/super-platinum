use super::*;

pub(super) async fn stage(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !data::enabled(&state.peek()) {
        return Err("Underline annotation lost original owner".into());
    }
    let ordinal = stages.len();
    let before = data::proof(&state.peek());
    let local = journal(state).await?;
    let runtime = model(&state.peek());
    checked(desktop, r#"(()=>{const n=document.querySelector('.canvas-comment-reply-version')||document.querySelector('.canvas-comment-author')||document.querySelector('.canvas-comment')||document.querySelector('[data-canvas-text-section=paragraph]');if(!n)return {ok:false};n.scrollIntoView({block:'nearest'});return {ok:true};})()"#).await?;
    let geometry = wait(desktop, include_str!("geometry.js")).await?;
    if data::proof(&state.peek()) != before || model(&state.peek()) != runtime {
        return Err("Annotation changed during geometry".into());
    }
    let receipt = json!({"ok":true,"stage":name,"ordinal":ordinal,"noSession":true,"noTransport":true,"fixture":before,"model":runtime,"journal":local,"geometry":geometry});
    let directory = std::env::var_os("SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR")
        .ok_or("Missing underline capture ACK directory")?;
    let directory = std::path::PathBuf::from(directory);
    if !directory.is_absolute() {
        return Err("Underline capture directory must be absolute".into());
    }
    eprintln!("canvas-comment-underline-stage: {receipt}");
    let ack = directory.join(format!("{ordinal}.ack"));
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let path = ack.clone();
        match tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| "Underline capture ACK worker failed")?
        {
            Ok(v) if v == b"captured\n" => break,
            Ok(_) => return Err("Malformed underline capture ACK".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("Underline capture ACK read failed".into()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("Underline capture timed out: {name}"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let after = checked(desktop, include_str!("geometry.js")).await?;
    if geometry != after
        || data::proof(&state.peek()) != before
        || model(&state.peek()) != runtime
        || journal(state).await? != local
    {
        return Err(format!("Underline changed during {name} screenshot"));
    }
    stages.push(receipt);
    Ok(())
}
