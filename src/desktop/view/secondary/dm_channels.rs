use std::collections::HashMap;

use crate::model::ChannelVm;

/// Borrow only for this render: presence and media changes are immediately
/// visible without a retained cache or a second invalidation boundary. Keep the
/// first duplicate, matching the previous per-row `iter().find` behavior.
pub(super) fn index(channels: &[ChannelVm]) -> HashMap<&str, &ChannelVm> {
    let mut index = HashMap::with_capacity(channels.len());
    for channel in channels {
        index.entry(channel.id.as_str()).or_insert(channel);
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PresenceVm;

    #[test]
    fn lookup_preserves_first_match_without_assuming_sorted_or_complete_rows() {
        let channels = vec![
            ChannelVm {
                id: "DZ".into(),
                presence: PresenceVm::Away,
                ..Default::default()
            },
            ChannelVm {
                id: "DA".into(),
                presence: PresenceVm::Active,
                ..Default::default()
            },
            ChannelVm {
                id: "DZ".into(),
                presence: PresenceVm::Active,
                ..Default::default()
            },
        ];
        let indexed = index(&channels);
        for id in ["DA", "DZ", "missing"] {
            let expected = channels.iter().find(|channel| channel.id == id);
            let actual = indexed.get(id).copied();
            assert_eq!(actual, expected);
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!(
                    std::ptr::eq(actual, expected),
                    "borrow original row, including first duplicate"
                );
            }
        }
        assert_eq!(indexed.get("DZ").unwrap().presence, PresenceVm::Away);
        assert!(!indexed.contains_key("missing"));
    }
}
