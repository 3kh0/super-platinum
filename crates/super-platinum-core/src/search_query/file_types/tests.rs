use super::*;
use crate::search_query::SearchFilters;

#[test]
fn source_categories_and_explicit_aliases_project_and_emit_only_the_three_plural_tokens() {
    for (kind, alias) in [
        (FileCategory::Documents, "document"),
        (FileCategory::Images, "image"),
        (FileCategory::Pdfs, "pdf"),
    ] {
        let q = format!("release type:{alias} type:{}", kind.value());
        let edit = FileTypeEdit::capture(&q);
        assert!(edit.selected(kind));
        assert_eq!(
            edit.apply(&q).unwrap(),
            q,
            "no-op retains aliases/duplicates byteexact"
        );
        let mut edit = FileTypeEdit::capture("release");
        edit.set(kind, true);
        assert_eq!(
            edit.apply("release").unwrap(),
            format!("release type:{}", kind.value())
        );
    }
    let mut edit = FileTypeEdit::capture("release");
    for kind in FileCategory::ALL {
        edit.set(kind, true);
    }
    assert_eq!(
        edit.apply("release").unwrap(),
        "release type:documents type:images type:pdfs"
    );
}
#[test]
fn changed_selection_and_scoped_clear_preserve_exclusions_unknowns_quotes_localized_terms_and_unicode_bytes()
 {
    let q = "café\ttype:image  -type:pdfs \"type:documents\" type:videos tipo:images\nfrom:<@U1>";
    let mut edit = FileTypeEdit::capture(q);
    assert!(edit.selected(FileCategory::Images));
    assert!(!edit.selected(FileCategory::Documents));
    edit.set(FileCategory::Images, false);
    edit.set(FileCategory::Pdfs, true);
    assert_eq!(
        edit.apply(q).unwrap(),
        "café\t  -type:pdfs \"type:documents\" type:videos tipo:images\nfrom:<@U1> type:pdfs"
    );
    edit.clear_selected();
    assert_eq!(
        edit.apply(q).unwrap(),
        "café\t  -type:pdfs \"type:documents\" type:videos tipo:images\nfrom:<@U1>"
    );
    let opaque = "type:\"images\" -type:pdf type:video \"type:image\"";
    let mut edit = FileTypeEdit::capture(opaque);
    edit.clear_selected();
    assert_eq!(edit.apply(opaque).unwrap(), opaque);
    assert!(edit.apply("a successor query").is_err());
}
#[test]
fn original_unfinished_quote_refuses_addition_before_other_filter_clear_can_hide_it() {
    let q = "type:pdf in:\"unfinished";
    let mut edit = FileTypeEdit::capture(q);
    edit.clear_selected();
    edit.set(FileCategory::Images, true);
    assert_eq!(
        edit.apply(q),
        Err("Close the quoted phrase before adding a filter.")
    );
    let mut generic = SearchFilters::from_query(q);
    generic.channel.clear();
    assert!(generic.apply(q).is_ok(), "generic clear alone is allowed");
    assert!(
        edit.apply(q).is_err(),
        "combined editor validates types on original text first"
    );
    edit.clear_selected();
    assert_eq!(edit.apply(q).unwrap(), " in:\"unfinished");
    let q = "type:images type:pdfs in:\"unfinished";
    let mut removal = FileTypeEdit::capture(q);
    removal.set(FileCategory::Images, false);
    assert!(removal.selected(FileCategory::Pdfs));
    assert_eq!(
        removal.apply(q),
        Err("Close the quoted phrase before adding a filter.")
    );
    removal.clear_selected();
    assert_eq!(removal.apply(q).unwrap(), "  in:\"unfinished");
}
#[test]
fn finite_type_delta_composes_with_existing_filters_without_owning_generic_message_type_syntax() {
    let q = "release in:ship -type:pdfs type:videos";
    let mut edit = FileTypeEdit::capture(q);
    edit.set(FileCategory::Pdfs, true);
    let mut generic = SearchFilters::from_query(q);
    generic.channel = "design".into();
    generic.on = "2026-10-29".into();
    generic.has_file = true;
    let types = edit.apply(q).unwrap();
    let combined = generic.apply(&types).unwrap();
    assert!(
        combined.contains("type:videos")
            && combined.contains("-type:pdfs")
            && combined.contains("type:pdfs")
    );
    assert!(
        combined.contains("in:design")
            && combined.contains("on:2026-10-29")
            && combined.contains("has:file")
    );
    assert!(!combined.contains("in:ship"));
    let generic = SearchFilters::from_query(q);
    assert_eq!(
        generic.apply(q).unwrap(),
        q,
        "ordinary generic callers still own no type syntax"
    );
}
