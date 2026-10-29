//! Three represented positive file categories. Other query syntax stays owned
//! by its author; these controls confer no file or private-request authority.
use super::{tokens, unfinished_quote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCategory {
    Documents,
    Images,
    Pdfs,
}
impl FileCategory {
    pub const ALL: [Self; 3] = [Self::Documents, Self::Images, Self::Pdfs];
    pub fn label(self) -> &'static str {
        match self {
            Self::Documents => "Documents",
            Self::Images => "Images",
            Self::Pdfs => "PDFs",
        }
    }
    pub fn value(self) -> &'static str {
        match self {
            Self::Documents => "documents",
            Self::Images => "images",
            Self::Pdfs => "pdfs",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Documents => 0,
            Self::Images => 1,
            Self::Pdfs => 2,
        }
    }
    fn token(raw: &str) -> Option<Self> {
        match raw {
            "type:documents" | "type:document" => Some(Self::Documents),
            "type:images" | "type:image" => Some(Self::Images),
            "type:pdfs" | "type:pdf" => Some(Self::Pdfs),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTypeEdit {
    original: String,
    baseline: [bool; 3],
    selected: [bool; 3],
}
impl FileTypeEdit {
    pub fn capture(query: &str) -> Self {
        let mut baseline = [false; 3];
        for (start, end) in tokens(query) {
            if let Some(kind) = FileCategory::token(&query[start..end]) {
                baseline[kind.index()] = true;
            }
        }
        Self {
            original: query.into(),
            baseline,
            selected: baseline,
        }
    }
    pub fn selected(&self, kind: FileCategory) -> bool {
        self.selected[kind.index()]
    }
    pub fn set(&mut self, kind: FileCategory, value: bool) {
        self.selected[kind.index()] = value;
    }
    /// Scoped clearing owns only the three represented positive categories.
    pub fn clear_selected(&mut self) {
        self.selected = [false; 3];
    }
    pub fn apply(&self, current_query: &str) -> Result<String, &'static str> {
        if current_query != self.original {
            return Err("Search changed. Reopen filters.");
        }
        if self.selected == self.baseline {
            return Ok(current_query.to_owned());
        }
        // Every retained category is re-appended by a changed selection too.
        // Even a removal-only change must not append it into an open phrase.
        if self.selected.iter().any(|selected| *selected) && unfinished_quote(&self.original) {
            return Err("Close the quoted phrase before adding a filter.");
        }
        let mut result = self.original.clone();
        for (start, end) in tokens(&self.original).into_iter().rev() {
            if FileCategory::token(&self.original[start..end]).is_some() {
                result.replace_range(start..end, "");
            }
        }
        for kind in FileCategory::ALL.into_iter().filter(|k| self.selected(*k)) {
            if result.chars().last().is_some_and(|c| !c.is_whitespace()) {
                result.push(' ');
            }
            result.push_str("type:");
            result.push_str(kind.value());
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
