use super::*;
mod admission;
mod counterpart;
mod editing;
pub(super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    match case().as_str() {
        "editing" => editing::run(s, d, n, st, cap).await,
        "counterpart" => counterpart::run(s, d, n, st, cap).await,
        "admission" => admission::run(s, d, n, st, cap).await,
        _ => Err("Unknown Datetime editing case".into()),
    }
}
