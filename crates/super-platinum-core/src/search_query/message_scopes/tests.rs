use super::*;
const C1: &str = "C12345678";
const C2: &str = "C87654321";
const U1: &str = "U12345678";
const U2: &str = "W87654321";
fn choices() -> Vec<ScopeChoice> {
    vec![
        ScopeChoice::channel(C1, ">ship>").unwrap(),
        ScopeChoice::channel(C2, "design").unwrap(),
        ScopeChoice::author(U1, "Maya").unwrap(),
        ScopeChoice::author(U2, "Maya").unwrap(),
    ]
}
#[test]
fn two_channels_and_authors_apply_with_other_fields_once_using_exact_emitted_grammar() {
    let mut edit = ScopeEdit::capture("release", choices()).unwrap();
    for (kind, id) in [
        (Scope::Channels, C1),
        (Scope::Channels, C2),
        (Scope::Authors, U1),
        (Scope::Authors, U2),
    ] {
        edit.set(kind, id, true).unwrap();
    }
    let mut fields = edit.fields();
    fields.with_person = "@Maya Chen".into();
    fields.on = "2024-02-29".into();
    fields.has_file = true;
    fields.in_thread = true;
    assert_eq!(
        edit.apply("release", &fields).unwrap(),
        "release in:<#C12345678|ship> in:<#C87654321|design> from:<@U12345678> from:<@W87654321> with:\"@Maya Chen\" on:2024-02-29 has:file is:thread"
    );
    assert_eq!(
        edit.selected_count(Scope::Authors),
        2,
        "duplicate display labels keep distinct IDs"
    );
    assert!(edit.set(Scope::Authors, "UUNKNOWN1", true).is_err());
    assert!(edit.apply("successor query", &fields).is_err());
}
#[test]
fn no_op_retains_raw_occurrences_and_removal_owns_only_known_positive_emitted_identity_forms() {
    let query = "  ?\tin:<#C12345678|space on:2026-01-01 from:me> in:<#C12345678> -in:<#C87654321|raw> in:<#CUNKNOWN1|unknown> in:ship from:<@U12345678> from:<@U12345678|Maya> -from:<@W87654321> \"in:<#C87654321>\" after:yesterday ";
    let mut edit = ScopeEdit::capture(query, choices()).unwrap();
    let fields = edit.fields();
    assert_eq!(fields.on, "");
    assert_eq!(fields.sender, "");
    assert_eq!(fields.after, "yesterday");
    assert_eq!(edit.apply(query, &fields).unwrap(), query);
    edit.set(Scope::Channels, C1, false).unwrap();
    edit.set(Scope::Authors, U1, false).unwrap();
    assert_eq!(
        edit.apply(query, &fields).unwrap(),
        "  ?\t  -in:<#C87654321|raw> in:<#CUNKNOWN1|unknown> in:ship  from:<@U12345678|Maya> -from:<@W87654321> \"in:<#C87654321>\" after:yesterday "
    );
    edit.set(Scope::Channels, C1, true).unwrap();
    edit.set(Scope::Authors, U1, true).unwrap();
    assert_eq!(
        edit.apply(query, &fields).unwrap(),
        query,
        "reselect retains original spelling/order/duplicates"
    );
}
#[test]
fn opaque_labels_and_unmanaged_scopes_cannot_expose_nested_date_person_or_flag_modifiers() {
    let raw = "in:<#C12345678|on:2020-01-01 from:me with:Bob> x:<unknown|on:2021-01-01 has:file> -in:<#C87654321|before:2025-01-01> ????:<#C87654321|has:link> in:\\<#C87654321|on:2022-01-01>";
    let query =
        format!("release {raw} \"on:quoted\" on:yesterday with:\"Old Name\" has:pin -has:link");
    let edit = ScopeEdit::capture(&query, choices()).unwrap();
    let mut fields = edit.fields();
    assert_eq!(fields.on, "yesterday");
    assert_eq!(fields.with_person, "Old Name");
    assert!(!fields.has_file && !fields.has_link);
    assert!(fields.before.is_empty());
    fields.on = "2024-02-29".into();
    fields.with_person = "New Person".into();
    fields.pinned = false;
    fields.saved = true;
    let result = edit.apply(&query, &fields).unwrap();
    assert!(result.starts_with(&format!("release {raw} \"on:quoted\" ")));
    assert!(result.ends_with("-has:link with:\"New Person\" on:2024-02-29 is:saved"));
    assert!(!result.contains("with:\"Old Name\""));
    let empty = ScopeEdit::capture("", choices()).unwrap();
    let mut unsafe_text = empty.fields();
    unsafe_text.with_person = "<unfinished".into();
    unsafe_text.has_file = true;
    assert!(
        empty.apply("", &unsafe_text).is_err(),
        "a new text value cannot absorb another appended clause"
    );
    fields.on = "2026-02-29".into();
    assert!(edit.apply(&query, &fields).is_err());
    assert_eq!(edit.apply(&query, &edit.fields()).unwrap(), query);
}
#[test]
fn original_unfinished_quotes_and_references_refuse_every_addition_before_clear_can_hide_them() {
    let opaque_quote = r#"release x:<opaque|"> "unfinished"#;
    let mut mixed = ScopeEdit::capture(opaque_quote, choices()).unwrap();
    assert!(
        !super::super::unfinished_quote(opaque_quote),
        "legacy quote count cancels while the new lexer still has an unfinished phrase"
    );
    mixed.set(Scope::Authors, U1, true).unwrap();
    assert_eq!(
        mixed.apply(opaque_quote, &mixed.fields()),
        Err("Close the quoted phrase before adding a filter.")
    );

    let query = "release in:<#C12345678> on:\"unfinished";
    let mut edit = ScopeEdit::capture(query, choices()).unwrap();
    edit.clear(Scope::Channels);
    let mut fields = edit.fields();
    fields.on.clear();
    fields.has_file = true;
    assert_eq!(
        edit.apply(query, &fields),
        Err("Close the quoted phrase before adding a filter.")
    );
    fields.has_file = false;
    edit.set(Scope::Authors, U1, true).unwrap();
    assert!(edit.apply(query, &fields).is_err());
    edit.clear(Scope::Authors);
    assert_eq!(
        edit.apply(query, &fields).unwrap(),
        "release  ",
        "clear-only remains safe"
    );
    let query = "release in:<#C12345678|not closed on:2026-01-01";
    let mut edit = ScopeEdit::capture(query, choices()).unwrap();
    let mut fields = edit.fields();
    assert!(fields.on.is_empty());
    assert_eq!(edit.apply(query, &fields).unwrap(), query);
    edit.set(Scope::Authors, U1, true).unwrap();
    assert_eq!(
        edit.apply(query, &fields),
        Err("Close the reference before adding a filter.")
    );
    edit.clear(Scope::Authors);
    fields.with_person = "Maya".into();
    assert!(edit.apply(query, &fields).is_err());
}
#[test]
fn selection_has_no_thirty_row_cap_and_scoped_clear_preserves_the_other_scope() {
    let mut all = Vec::new();
    for i in 0..40 {
        all.push(ScopeChoice::channel(format!("C{i:08}"), format!("c{i}")).unwrap());
        all.push(ScopeChoice::author(format!("U{i:08}"), format!("u{i}")).unwrap());
    }
    let query = "in:<#C00000000|original> in:<#C00000000> from:<@U00000000>";
    let mut edit = ScopeEdit::capture(query, all).unwrap();
    let fields = edit.fields();
    for i in 0..40 {
        edit.set(Scope::Channels, &format!("C{i:08}"), true)
            .unwrap();
        edit.set(Scope::Authors, &format!("U{i:08}"), true).unwrap();
    }
    assert_eq!(edit.selected_count(Scope::Channels), 40);
    assert_eq!(edit.selected_count(Scope::Authors), 40);
    let applied = edit.apply(query, &fields).unwrap();
    assert_eq!(
        applied
            .split_whitespace()
            .filter(|s| s.starts_with("in:<#"))
            .count(),
        41
    );
    assert_eq!(
        applied
            .split_whitespace()
            .filter(|s| s.starts_with("from:<@"))
            .count(),
        40
    );
    edit.clear(Scope::Authors);
    let channels = edit.apply(query, &fields).unwrap();
    assert!(!channels.contains("from:<@"));
    assert!(channels.starts_with("in:<#C00000000|original> in:<#C00000000> "));
    assert_eq!(edit.selected_count(Scope::Channels), 40);
    edit.clear(Scope::Channels);
    assert_eq!(edit.apply(query, &fields).unwrap(), "  ");
    let mut unexpected = fields;
    unexpected.channel = "ship".into();
    assert!(edit.apply(query, &unexpected).is_err());
}
#[test]
fn new_canonical_name_formatting_never_rewrites_retained_references_or_accepts_ambiguous_candidates()
 {
    assert!(ScopeChoice::channel("C1", "short").is_none());
    assert!(ScopeChoice::author("<@U12345678>", "markup").is_none());
    assert!(
        ScopeEdit::capture(
            "release",
            vec![
                ScopeChoice::channel(C1, "one").unwrap(),
                ScopeChoice::channel(C1, "two").unwrap()
            ]
        )
        .is_err()
    );
    let unsafe_choice = ScopeChoice::channel(C1, "unsafe\"name").unwrap();
    let query = "in:<#C12345678|retained>";
    let mut retained = ScopeEdit::capture(query, vec![unsafe_choice.clone()]).unwrap();
    assert_eq!(retained.apply(query, &retained.fields()).unwrap(), query);
    retained.clear(Scope::Channels);
    assert_eq!(retained.apply(query, &retained.fields()).unwrap(), "");
    let mut new = ScopeEdit::capture("", vec![unsafe_choice]).unwrap();
    new.set(Scope::Channels, C1, true).unwrap();
    assert!(new.apply("", &new.fields()).is_err());
    let mut empty_name = ScopeEdit::capture(
        "",
        vec![
            ScopeChoice::channel(C1, ">>>").unwrap(),
            ScopeChoice::author("USLACKBOT", "Slackbot").unwrap(),
        ],
    )
    .unwrap();
    empty_name.set(Scope::Channels, C1, true).unwrap();
    empty_name.set(Scope::Authors, "USLACKBOT", true).unwrap();
    assert_eq!(
        empty_name.apply("", &empty_name.fields()).unwrap(),
        "in:<#C12345678> from:<@USLACKBOT>"
    );
}
