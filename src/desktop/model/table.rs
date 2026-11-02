//! Passive table content only. No owner, action, URI, media handle or edit codec.
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Align {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Column {
    pub align: Align,
    pub wrapped: bool,
}
impl Column {
    pub fn class(self) -> &'static str {
        match (self.align, self.wrapped) {
            (Align::Left, false) => "block-table-cell left nowrap",
            (Align::Left, true) => "block-table-cell left wrapped",
            (Align::Center, false) => "block-table-cell center nowrap",
            (Align::Center, true) => "block-table-cell center wrapped",
            (Align::Right, false) => "block-table-cell right nowrap",
            (Align::Right, true) => "block-table-cell right wrapped",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Run {
    pub text: String,
    pub style: Style,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Paragraph(Vec<Run>),
    Quote(Vec<Run>),
    Code(String),
    List {
        ordered: bool,
        indent: u8,
        offset: u32,
        items: Vec<Vec<Run>>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell(pub Vec<Part>);
impl Cell {
    pub fn plain_text(&self) -> String {
        let runs = |r: &[Run]| r.iter().map(|r| r.text.as_str()).collect::<String>();
        self.0
            .iter()
            .map(|p| match p {
                Part::Paragraph(r) | Part::Quote(r) => runs(r),
                Part::Code(s) => s.clone(),
                Part::List {
                    ordered,
                    offset,
                    items,
                    ..
                } => items
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        format!(
                            "{}{}",
                            if *ordered {
                                format!("{}. ", u64::from(*offset) + i as u64 + 1)
                            } else {
                                "- ".into()
                            },
                            runs(r)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[derive(Debug, PartialEq, Eq)]
struct Data {
    columns: Vec<Column>,
    rows: Vec<Vec<Cell>>,
}
/// Immutable supplied reading data. Constructing it grants no editing or effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTable(Arc<Data>);
impl ReadTable {
    pub(crate) fn new(columns: Vec<Column>, rows: Vec<Vec<Cell>>) -> Self {
        Self(Arc::new(Data { columns, rows }))
    }
    pub(crate) fn rows(&self) -> &[Vec<Cell>] {
        &self.0.rows
    }
    pub(crate) fn column(&self, index: usize) -> Column {
        self.0.columns.get(index).copied().unwrap_or_default()
    }
    /// Reading fallback only; no Table writer or Source Copy/Download contract.
    pub(crate) fn plain_text(&self) -> String {
        self.rows()
            .iter()
            .map(|row| {
                row.iter()
                    .map(Cell::plain_text)
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
