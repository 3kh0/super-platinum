//! Event-free table reader. This renderer accepts no Signal, owner or media epoch.
use crate::model::table::{Cell, Part, ReadTable, Run};
use dioxus::prelude::*;

pub(super) fn render(table: &ReadTable) -> Element {
    let Some(header) = table.rows().first() else {
        return rsx! {};
    };
    rsx! {
        div { class:"block-table-scroll", role:"region", "aria-label":"Message table", tabindex:"0",
            table { class:"block-table",
                thead { tr {
                    for (column, cell) in header.iter().enumerate() {
                        th { key:"header-{column}", scope:"col", class:table.column(column).class(),
                            {content(cell)}
                        }
                    }
                }}
                tbody {
                    for (row_index, row) in table.rows().iter().enumerate().skip(1) {
                        tr { key:"row-{row_index}",
                            for (column, cell) in row.iter().enumerate() {
                                td { key:"cell-{column}", class:table.column(column).class(),
                                    {content(cell)}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
fn content(cell: &Cell) -> Element {
    rsx! { div { class:"block-table-cell-content",
        for (index, part) in cell.0.iter().enumerate() {
            div { key:"part-{index}", class:"block-table-part", {part_view(part)} }
        }
    }}
}
fn part_view(part: &Part) -> Element {
    match part {
        Part::Paragraph(runs) => rsx! { div { class:"block-table-line", {runs_view(runs)} } },
        Part::Quote(runs) => rsx! { blockquote { class:"block-table-quote", {runs_view(runs)} } },
        Part::Code(text) => rsx! { pre { class:"block-table-code", code { "{text}" } } },
        Part::List {
            ordered,
            indent,
            offset,
            items,
        } => {
            let style = format!("--list-indent:{indent}");
            if *ordered {
                rsx! { ol { class:"block-list block-table-list", style, start:(u64::from(*offset)+1).to_string(),
                    for (index, runs) in items.iter().enumerate() { li { key:"item-{index}", {runs_view(runs)} } }
                }}
            } else {
                rsx! { ul { class:"block-list block-table-list", style,
                    for (index, runs) in items.iter().enumerate() { li { key:"item-{index}", {runs_view(runs)} } }
                }}
            }
        }
    }
}
fn runs_view(runs: &[Run]) -> Element {
    rsx! { for (index, run) in runs.iter().enumerate() {
        span { key:"run-{index}", {run_view(run)} }
    }}
}
fn run_view(run: &Run) -> Element {
    // Supplied literal text is escaped by RSX, never inserted as HTML. These
    // semantic wrappers follow the qualified inner-to-outer formatting order.
    let mut node = rsx! { span { "{run.text}" } };
    if run.style.strike {
        node = rsx! { s { {node} } };
    }
    if run.style.italic {
        node = rsx! { i { {node} } };
    }
    if run.style.underline {
        node = rsx! { u { {node} } };
    }
    if run.style.bold {
        node = rsx! { b { {node} } };
    }
    if run.style.code {
        node = rsx! { code { class:"block-table-inline-code", {node} } };
    }
    node
}
