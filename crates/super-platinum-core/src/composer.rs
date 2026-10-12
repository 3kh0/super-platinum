#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    pub fn new(anchor: usize, focus: usize) -> Self {
        Self {
            start: anchor.min(focus),
            end: anchor.max(focus),
        }
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMark {
    Bold,
    Italic,
    Strike,
    Code,
    CodeBlock,
    Quote,
}

impl FormatMark {
    fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::Bold => ("*", "*"),
            Self::Italic => ("_", "_"),
            Self::Strike => ("~", "~"),
            Self::Code => ("`", "`"),
            Self::CodeBlock => ("```\n", "\n```"),
            Self::Quote => ("> ", ""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct ComposerState {
    pub text: String,
    pub selection: TextRange,
}

impl ComposerState {
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            selection: TextRange::new(end, end),
        }
    }

    pub fn set_selection(&mut self, anchor: usize, focus: usize) {
        self.selection = self.valid_range(TextRange::new(anchor, focus));
    }

    pub fn replace_selection(&mut self, replacement: &str) {
        let range = self.valid_range(self.selection);
        self.text.replace_range(range.start..range.end, replacement);
        let cursor = range.start + replacement.len();
        self.selection = TextRange::new(cursor, cursor);
    }

    pub fn apply_format(&mut self, mark: FormatMark) {
        let range = self.valid_range(self.selection);
        let (before, after) = mark.delimiters();
        if mark == FormatMark::Quote {
            self.toggle_quote(range);
            return;
        }
        // A toolbar or shortcut is a toggle. Preserve the selection inside the
        // delimiters so pressing Bold twice restores the original draft.
        if range.start >= before.len()
            && self.text[..range.start].ends_with(before)
            && self.text[range.end..].starts_with(after)
        {
            self.text
                .replace_range(range.end..range.end + after.len(), "");
            self.text
                .replace_range(range.start - before.len()..range.start, "");
            self.selection = TextRange::new(range.start - before.len(), range.end - before.len());
            return;
        }
        let selected = self.text[range.start..range.end].to_owned();
        let replacement = format!("{before}{selected}{after}");
        self.text
            .replace_range(range.start..range.end, &replacement);
        self.selection = if range.is_empty() {
            let cursor = range.start + before.len();
            TextRange::new(cursor, cursor)
        } else {
            TextRange::new(range.start + before.len(), range.end + before.len())
        };
    }

    fn toggle_quote(&mut self, range: TextRange) {
        let start = self.text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        // Selecting through a line break does not include the following line.
        let selected_end = if range.end > range.start && self.text[..range.end].ends_with('\n') {
            range.end - 1
        } else {
            range.end
        };
        let end = self.text[selected_end..]
            .find('\n')
            .map_or(self.text.len(), |i| selected_end + i);
        let lines: Vec<_> = self.text[start..end].split('\n').collect();
        let remove = lines.iter().all(|line| line.starts_with("> "));
        let replacement = lines
            .iter()
            .map(|line| {
                if remove {
                    line[2..].to_owned()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.text.replace_range(start..end, &replacement);
        self.selection = if range.is_empty() {
            let cursor = if remove {
                range.start.saturating_sub(2).max(start)
            } else {
                range.start + 2
            };
            TextRange::new(cursor, cursor)
        } else {
            TextRange::new(start, start + replacement.len())
        };
    }

    fn valid_range(&self, mut range: TextRange) -> TextRange {
        range.start = previous_char_boundary(&self.text, range.start.min(self.text.len()));
        range.end = previous_char_boundary(&self.text, range.end.min(self.text.len()));
        range
    }
}

fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_preserves_selected_text_and_selection() {
        let mut composer = ComposerState::with_text("ship it");
        composer.set_selection(0, 4);
        composer.apply_format(FormatMark::Bold);
        assert_eq!(composer.text, "*ship* it");
        assert_eq!(composer.selection, TextRange::new(1, 5));
    }

    #[test]
    fn selection_is_clamped_to_utf8_boundaries() {
        let mut composer = ComposerState::with_text("a🚀b");
        composer.set_selection(2, usize::MAX);
        assert_eq!(composer.selection, TextRange::new(1, 6));
        composer.replace_selection("!");
        assert_eq!(composer.text, "a!");
    }

    #[test]
    fn inline_formats_toggle_without_losing_unicode_or_selection() {
        for mark in [
            FormatMark::Bold,
            FormatMark::Italic,
            FormatMark::Strike,
            FormatMark::Code,
            FormatMark::CodeBlock,
        ] {
            let mut composer = ComposerState::with_text("hi 🚀 ship it");
            composer.set_selection(3, 10);
            let original = composer.clone();
            composer.apply_format(mark);
            composer.apply_format(mark);
            assert_eq!(composer, original);
        }
    }

    #[test]
    fn empty_format_toggle_keeps_insertion_point() {
        let mut composer = ComposerState::with_text("hi ");
        let original = composer.clone();
        composer.apply_format(FormatMark::Bold);
        assert_eq!(composer.text, "hi **");
        assert_eq!(composer.selection, TextRange::new(4, 4));
        composer.apply_format(FormatMark::Bold);
        assert_eq!(composer, original);
    }

    #[test]
    fn quote_formats_each_selected_line_without_touching_the_next() {
        let mut composer = ComposerState::with_text("one\ntwo\nthree");
        composer.set_selection(1, 8);
        composer.apply_format(FormatMark::Quote);
        assert_eq!(composer.text, "> one\n> two\nthree");
        composer.apply_format(FormatMark::Quote);
        assert_eq!(composer.text, "one\ntwo\nthree");
    }
}
