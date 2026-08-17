#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
}
