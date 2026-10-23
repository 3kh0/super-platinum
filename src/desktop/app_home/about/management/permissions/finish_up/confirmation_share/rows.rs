//! Selector projection only: ranking/filtering never creates destination authority.
//! A known DM hides its person only while that DM remains displayed.
type Row = (String, String);
type Conversation = (String, String, Option<String>);

pub(super) fn project(
    conversations: &[Conversation],
    people: &[Row],
    selected: &str,
    pinned: Option<&Row>,
    ranked: Option<&[String]>,
) -> Vec<Row> {
    let displayed =
        |id: &str| id == selected || ranked.is_none_or(|ids| ids.iter().any(|v| v == id));
    let mut rows = conversations
        .iter()
        .map(|(id, label, _)| (id.clone(), label.clone()))
        .collect::<Vec<_>>();
    rows.extend(
        people
            .iter()
            .filter(|(person, _)| {
                !conversations
                    .iter()
                    .any(|(id, _, peer)| peer.as_ref() == Some(person) && displayed(id))
            })
            .cloned(),
    );
    if !selected.is_empty() && !rows.iter().any(|(id, _)| id == selected) {
        if let Some(row) = pinned.filter(|(id, _)| id == selected) {
            rows.push(row.clone());
        }
    }
    if let Some(ids) = ranked {
        rows.retain(|(id, _)| displayed(id));
        rows.sort_by_key(|(id, _)| ids.iter().position(|v| v == id).unwrap_or(usize::MAX));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str) -> Row {
        (id.into(), format!("label {id}"))
    }
    fn conversation(id: &str, peer: Option<&str>) -> Conversation {
        (id.into(), format!("label {id}"), peer.map(str::to_owned))
    }
    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn share_recipient_rows_ranked_person_survives_unranked_mapped_dm() {
        assert_eq!(
            project(
                &[conversation("D1", Some("U1"))],
                &[row("U1"), row("U2")],
                "",
                None,
                Some(&ids(&["U2", "U1"]))
            ),
            vec![row("U2"), row("U1")]
        );
    }
    #[test]
    fn share_recipient_rows_selected_dm_deduplicates_person_and_stays_pinned() {
        assert_eq!(
            project(
                &[conversation("D1", Some("U1"))],
                &[row("U1"), row("U2")],
                "D1",
                None,
                Some(&ids(&["U1", "U2"]))
            ),
            vec![row("U2"), row("D1")]
        );
    }
    #[test]
    fn share_recipient_rows_mixed_order_and_ranked_dm_keep_original_identity() {
        let conversations = [
            conversation("C1", None),
            conversation("D1", Some("U1")),
            conversation("C2", None),
        ];
        assert_eq!(
            project(
                &conversations,
                &[row("U1"), row("U2")],
                "",
                None,
                Some(&ids(&["C2", "U2", "D1", "U1", "C1"]))
            ),
            vec![row("C2"), row("U2"), row("D1"), row("C1")]
        );
    }
    #[test]
    fn share_recipient_rows_resolving_person_uses_only_exact_selected_pin() {
        let conversations = [conversation("D1", Some("U1"))];
        assert_eq!(
            project(&conversations, &[], "U1", Some(&row("U1")), Some(&[])),
            vec![row("U1")]
        );
        assert!(project(&conversations, &[], "U1", Some(&row("U2")), Some(&[])).is_empty());
    }
    #[test]
    fn share_recipient_rows_unfiltered_known_dm_preserves_previous_dedup() {
        assert_eq!(
            project(
                &[conversation("D1", Some("U1")), conversation("C1", None)],
                &[row("U1"), row("U2")],
                "",
                None,
                None
            ),
            vec![row("D1"), row("C1"), row("U2")]
        );
    }
}
