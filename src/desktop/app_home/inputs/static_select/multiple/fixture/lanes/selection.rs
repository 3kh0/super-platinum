use super::*;
pub(in super::super) async fn run(
    s: Signal<ShellState>,
    d: &DesktopContext,
    n: &str,
    st: &mut Vec<Value>,
    cap: &mut Captures,
) -> Result<(), String> {
    for (i, (stage, ops)) in [
        ("selected-beta", vec![toggle(1)]),
        ("ordered-beta-alpha", vec![toggle(0)]),
        ("removed-beta", vec![remove(0)]),
        ("readded-beta", vec![toggle(1)]),
        ("empty-value-option", vec![remove(0), remove(0), toggle(3)]),
        ("optional-empty", vec![remove(0)]),
    ]
    .into_iter()
    .enumerate()
    {
        let change = edit(s, d, "pick", ops).await?;
        counts(s, i + 1, i + 1, 0, 0).await?;
        shot(s, d, n, stage, json!({"change":change}), st, cap).await?;
    }
    super::props::run(s, d, n, st, cap).await
}
