use super::*;
use crate::search_query::{SearchFilters, message_scopes::ScopeEdit};
fn edit(query: &str) -> ScopeEdit {
    ScopeEdit::capture(query, Vec::new()).unwrap()
}

#[test]
fn untouched_reaction_syntax_is_exact_and_only_explicit_positive_codes_are_replaced() {
    let query = "déploiement has::eyes: has::thumbsup: hasmy::white_check_mark: -has::eyes: \"has::fire:\" in:<#C12345678|has::boom: hidden> has:pin has::bad.code:";
    let e = edit(query);
    let mut fields = e.fields();
    assert_eq!(fields.reactions.as_ref().unwrap().any, ":eyes:");
    assert_eq!(
        fields.reactions.as_ref().unwrap().mine,
        ":white_check_mark:"
    );
    assert_eq!(e.apply(query, &fields).unwrap(), query);
    fields.reactions.as_mut().unwrap().any = "rocket".into();
    fields.reactions.as_mut().unwrap().mine.clear();
    assert_eq!(
        e.apply(query, &fields).unwrap(),
        "déploiement    -has::eyes: \"has::fire:\" in:<#C12345678|has::boom: hidden> has:pin has::bad.code: has::rocket:"
    );
    let cleared = e.apply(query, &SearchFilters::default()).unwrap();
    assert_eq!(
        cleared,
        "déploiement    -has::eyes: \"has::fire:\" in:<#C12345678|has::boom: hidden>  has::bad.code:"
    );
    let generic = SearchFilters::from_query(query);
    assert!(generic.reactions.is_none());
    assert_eq!(
        generic.apply(query).unwrap(),
        query,
        "generic and Files syntax remains unmanaged"
    );
    assert!(
        fields.apply(query).is_err(),
        "typed intent cannot silently use generic submission"
    );
}

#[test]
fn both_reaction_keys_are_independent_and_invalid_codes_or_open_syntax_refuse_atomically() {
    let e = edit("release has:link");
    let mut fields = e.fields();
    fields.reactions = Some(Fields {
        any: ":eyes:".into(),
        mine: "+1".into(),
    });
    assert_eq!(
        e.apply("release has:link", &fields).unwrap(),
        "release has:link has::eyes: hasmy::+1:"
    );
    for value in [
        ":eyes",
        "eyes:",
        ":eye:s:",
        "eyes from:me",
        "étoile",
        "<eyes>",
        "\"eyes\"",
        "eyes\\",
        ":",
        "::",
    ] {
        fields.reactions.as_mut().unwrap().any = value.into();
        assert!(e.apply("release has:link", &fields).is_err(), "{value}");
    }
    fields.reactions.as_mut().unwrap().any = "x".repeat(101);
    assert!(e.apply("release has:link", &fields).is_err());
    for query in ["release \"unfinished", "release in:<#C12345678|unfinished"] {
        let e = edit(query);
        let mut fields = e.fields();
        fields.reactions = Some(Fields {
            any: "eyes".into(),
            mine: String::new(),
        });
        assert!(e.apply(query, &fields).is_err());
        assert_eq!(e.apply(query, &e.fields()).unwrap(), query);
    }
}

#[test]
fn clear_can_remove_owned_reactions_before_incomplete_quoted_text_without_rewriting_literals() {
    let query = "has::eyes: hasmy::eyes: -hasmy::eyes: \"unfinished has::fire:";
    let e = edit(query);
    assert_eq!(
        e.apply(query, &SearchFilters::default()).unwrap(),
        "  -hasmy::eyes: \"unfinished has::fire:"
    );
    let mut fields = e.fields();
    fields.reactions.as_mut().unwrap().mine = "rocket".into();
    assert!(e.apply(query, &fields).is_err());
}
