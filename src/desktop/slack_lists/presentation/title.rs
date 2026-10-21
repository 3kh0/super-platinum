//! Ordinary live List header: original canonical title and one L7LR pass.
//! Operational View/Document titles, including their existing fallbacks, stay
//! untouched. Loading uses only the original opener title.
use super::super::View;

pub(super) fn display(view: &View) -> String {
    let raw = match &view.loaded {
        Some(loaded) => loaded.document.raw.get("title").and_then(|v| v.as_str()),
        None => view.loading_title.as_deref(),
    };
    unescape(raw.unwrap_or_default())
}

fn unescape(mut input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    while let Some(index) = input.find('&') {
        output.push_str(&input[..index]);
        input = &input[index..];
        let entity = [
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
            ("&#39;", "'"),
        ]
        .into_iter()
        .find(|(encoded, _)| input.starts_with(encoded));
        if let Some((encoded, decoded)) = entity {
            output.push_str(decoded);
            input = &input[encoded.len()..];
        } else {
            output.push('&');
            input = &input[1..];
        }
    }
    output.push_str(input);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slack_lists::{Document, presentation, tests as fixtures};
    use serde_json::{Value, json};

    #[test]
    fn five_entities_once_preserve_other_entities_unicode_and_literal_text() {
        for (raw, expected) in [
            ("&amp; &lt; &gt; &quot; &#39;", "& < > \" '"),
            ("&amp;lt; &amp;amp; &amp;#39;", "&lt; &amp; &#39;"),
            (
                "&AMP; &nbsp; &#65; &#x41; &#039;",
                "&AMP; &nbsp; &#65; &#x41; &#039;",
            ),
            (
                "  R\u{e9}vision &amp; \u{4e2d} \u{1f98a}  ",
                "  R\u{e9}vision & \u{4e2d} \u{1f98a}  ",
            ),
            ("<img src=x> _list_ * &lt;b&gt;", "<img src=x> _list_ * <b>"),
            (" \t\n ", " \t\n "),
            ("", ""),
        ] {
            assert_eq!(unescape(raw), expected);
        }
    }

    #[test]
    fn loaded_original_title_wins_without_operational_or_loading_fallback() {
        for title in [
            None,
            Some(Value::Null),
            Some(json!("")),
            Some(json!(7)),
            Some(json!("  ")),
        ] {
            let mut shell = fixtures::make_shell();
            let view = shell.slack_lists.view.as_mut().unwrap();
            view.loading_title = Some("Opener &amp; title".into());
            view.title = "Operational title".into();
            let loaded = view.loaded.as_mut().unwrap();
            let mut raw = loaded.document.raw.clone();
            raw["name"] = json!("Must not become the heading");
            match &title {
                Some(value) => raw["title"] = value.clone(),
                None => {
                    raw.as_object_mut().unwrap().remove("title");
                }
            }
            loaded.document = Document::parse(raw).unwrap();
            let canonical = loaded.document.clone();
            let expected = title.as_ref().and_then(Value::as_str).unwrap_or_default();
            assert_eq!(presentation(&shell).unwrap().title, expected);
            let view = shell.slack_lists.view.as_ref().unwrap();
            assert_eq!(view.title, "Operational title");
            assert_eq!(view.loaded.as_ref().unwrap().document, canonical);
        }
    }

    #[test]
    fn opener_title_is_separate_and_loaded_projection_does_not_rewrite_source() {
        let mut shell = fixtures::make_shell();
        let mut file = fixtures::file();
        file.title = Some("Loading &amp;lt; title".into());
        assert!(crate::slack_lists::open_shell(&mut shell, file));
        assert_eq!(presentation(&shell).unwrap().title, "Loading &lt; title");
        let mut loaded = fixtures::loaded(false);
        loaded.document.raw["title"] = json!("Canonical &amp;lt; \u{4e2d}");
        loaded.document = Document::parse(loaded.document.raw.clone()).unwrap();
        let before = loaded.document.clone();
        fixtures::read(&mut shell, loaded);
        assert_eq!(
            presentation(&shell).unwrap().title,
            "Canonical &lt; \u{4e2d}"
        );
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(
            view.loading_title.as_deref(),
            Some("Loading &amp;lt; title")
        );
        assert_eq!(view.title, before.title);
        assert_eq!(view.loaded.as_ref().unwrap().document, before);
        assert_eq!(
            presentation(&shell).unwrap().title,
            "Canonical &lt; \u{4e2d}"
        );
        let mut untitled = fixtures::file();
        untitled.title = None;
        untitled.name = Some("Operational loading fallback only".into());
        assert!(crate::slack_lists::open_shell(&mut shell, untitled));
        assert_eq!(presentation(&shell).unwrap().title, "");
        assert!(
            !shell.slack_lists.view.as_ref().unwrap().title.is_empty(),
            "the existing operational fallback remains independent"
        );
    }

    #[test]
    fn heading_fragments_use_original_scope_and_preserve_semantic_and_canonical_titles() {
        use crate::emoji_text::Part;
        let mut shell = fixtures::make_shell();
        let team = shell.core.active_team.clone().unwrap();
        let workspace = shell.core.workspaces.get_mut(&team).unwrap();
        for (name, value) in [
            ("wave", "https://fixture.invalid/collision.png"),
            ("header_base", "https://fixture.invalid/header.png"),
            ("header_alias", "alias:header_base"),
        ] {
            workspace.custom_emoji.insert(
                name.into(),
                serde_json::from_value(json!({"name":name,"value":value})).unwrap(),
            );
        }
        let view = shell.slack_lists.view.as_mut().unwrap();
        let mut raw = view.loaded.as_ref().unwrap().document.raw.clone();
        raw["title"] = json!("R\u{e9}vision &amp; \u{4e2d} :wave: :header_alias: :unknown:");
        let canonical = Document::parse(raw).unwrap();
        view.loaded.as_mut().unwrap().document = canonical.clone();
        let operational = view.title.clone();
        let projected = presentation(&shell).unwrap();
        assert_eq!(
            projected.title,
            "R\u{e9}vision & \u{4e2d} :wave: :header_alias: :unknown:"
        );
        let parts = &projected.title_fragments;
        assert_eq!(parts.len(), 6);
        assert_eq!(parts[0], Part::Text("R\u{e9}vision & \u{4e2d} ".into()));
        assert_eq!(
            parts[1],
            Part::Emoji {
                text: "\u{1f44b}".into(),
                title: Some(":wave:".into()),
                image: None
            }
        );
        assert_eq!(parts[2], Part::Text(" ".into()));
        let Part::Emoji {
            text,
            title,
            image: Some(id),
        } = &parts[3]
        else {
            panic!("missing heading alias")
        };
        assert_eq!(text, ":header_alias:");
        assert_eq!(title.as_deref(), Some(":header_alias:"));
        assert!(!shell.media.is_ready(id));
        shell
            .media
            .insert(id.clone(), "image/svg+xml", b"<svg/>".to_vec());
        assert_eq!(presentation(&shell).unwrap().title_fragments, *parts);
        assert_eq!(parts[4], Part::Text(" ".into()));
        assert_eq!(parts[5], Part::Text(":unknown:".into()));
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(view.title, operational);
        assert_eq!(view.loaded.as_ref().unwrap().document, canonical);
        shell.core.active_team = Some("TFOREIGN".into());
        assert!(
            presentation(&shell).is_none(),
            "original scope cannot borrow another team's emoji"
        );
    }

    #[test]
    fn heading_scan_never_redecodes_the_selected_title() {
        let mut shell = fixtures::make_shell();
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.loaded.as_mut().unwrap().document.raw["title"] =
            json!(" &amp;lt;<b> :wave::skin-tone-3: ");
        let projected = presentation(&shell).unwrap();
        assert_eq!(projected.title, " &lt;<b> :wave::skin-tone-3: ");
        assert_eq!(
            projected.title_fragments,
            vec![
                crate::emoji_text::Part::Text(" &lt;<b> ".into()),
                crate::emoji_text::Part::Text(":wave::skin-tone-3:".into()),
                crate::emoji_text::Part::Text(" ".into()),
            ]
        );
    }
}
