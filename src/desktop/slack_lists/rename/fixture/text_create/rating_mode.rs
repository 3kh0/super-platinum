//! Existing exact Rating mode gate, unchanged assertions.
use super::*;
pub(super) fn rating_gate(
    flag: Option<&str>,
    exclusive: bool,
    alias: Option<&str>,
) -> Option<FieldCreateMode> {
    if flag != Some("1") || !exclusive {
        return None;
    }
    [
        FieldCreateMode::RatingDefault,
        FieldCreateMode::RatingStandard,
        FieldCreateMode::RatingCustom,
    ]
    .into_iter()
    .find(|m| alias == Some(m.prefix()) || alias == Some(format!("{}-narrow", m.prefix()).as_str()))
}

#[cfg(test)]
mod rating_gate_tests {
    use super::*;
    #[test]
    fn only_six_exact_rating_aliases_and_exclusive_one_opt_in_admit() {
        for mode in [
            FieldCreateMode::RatingDefault,
            FieldCreateMode::RatingStandard,
            FieldCreateMode::RatingCustom,
        ] {
            for alias in [
                mode.prefix().to_string(),
                format!("{}-narrow", mode.prefix()),
            ] {
                assert_eq!(rating_gate(Some("1"), true, Some(&alias)), Some(mode));
                assert_eq!(rating_gate(Some("1"), false, Some(&alias)), None);
                for flag in [None, Some(""), Some("0"), Some("true")] {
                    assert_eq!(rating_gate(flag, true, Some(&alias)), None);
                }
            }
        }
        for alias in [
            None,
            Some("slack-list-rating-field-create"),
            Some("slack-list-rating-cells"),
            Some("slack-list-rating-default-field-create-narrow-extra"),
        ] {
            assert_eq!(rating_gate(Some("1"), true, alias), None);
        }
    }
}
