//! Exact production comparison; standalone std-only tests can run without the renderer.
// Key, native node id, mount id, received sequence, received value.
pub(crate) fn compare<K: PartialEq>(
    old: &[(K, u64, u64, u64, String)],
    new: &[(K, u64, u64, u64, String)],
) -> Option<bool> {
    if old.len() != new.len() {
        return None;
    }
    let mut progressed = false;
    for (now, before) in new.iter().zip(old) {
        if now.0 != before.0
            || now.1 != before.1
            || now.2 != before.2
            || now.3 < before.3
            || (now.3 == before.3 && now.4 != before.4)
        {
            return None;
        }
        progressed |= now.3 > before.3;
    }
    Some(progressed)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Vec<(&'static str, u64, u64, u64, String)> {
        vec![("date", 1, 2, 3, "2026-10-29".into())]
    }
    #[test]
    fn unchanged_and_empty_are_not_progress() {
        assert_eq!(compare(&row(), &row()), Some(false));
        assert_eq!(compare::<&str>(&[], &[]), Some(false));
    }
    #[test]
    fn new_received_sequence_with_same_or_changed_value_is_progress() {
        let old = row();
        let mut new = row();
        new[0].3 += 1;
        assert_eq!(compare(&old, &new), Some(true));
        new[0].4 = "2026-10-30".into();
        assert_eq!(compare(&old, &new), Some(true));
    }
    #[test]
    fn value_without_sequence_and_sequence_regression_are_not_retryable() {
        let old = row();
        let mut new = row();
        new[0].4 = "2026-10-30".into();
        assert_eq!(compare(&old, &new), None);
        new[0].3 -= 1;
        assert_eq!(compare(&old, &new), None);
    }
    #[test]
    fn key_node_mount_and_inventory_replacement_refuse() {
        let old = row();
        for kind in 0..5 {
            let mut new = row();
            new[0].3 += 1;
            match kind {
                0 => new[0].0 = "other",
                1 => new[0].1 += 1,
                2 => new[0].2 += 1,
                3 => new.clear(),
                _ => new.push(("extra", 4, 5, 6, "".into())),
            };
            assert_eq!(compare(&old, &new), None);
        }
    }
    #[test]
    fn valid_progress_does_not_hide_another_changed_identity() {
        let mut old = row();
        old.push(("second", 4, 5, 6, "".into()));
        let mut new = old.clone();
        new[0].3 += 1;
        new[1].2 += 1;
        assert_eq!(compare(&old, &new), None);
    }
}
