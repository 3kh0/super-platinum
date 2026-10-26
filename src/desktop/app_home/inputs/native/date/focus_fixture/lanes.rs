use super::*;
mod editing;
mod focus;
mod guards;
mod lifecycle;
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    match case().as_str() {
        "focus" => focus::run(s, d, n, st, cap).await,
        "editing" => editing::run(s, d, n, st, cap).await,
        "lifecycle" => lifecycle::run(s, d, n, st, cap).await,
        "guards" => guards::run(s, d, n, st, cap).await,
        _ => Err("Unknown Date focus lane".into()),
    }
}
