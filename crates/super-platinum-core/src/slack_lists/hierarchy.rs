//! Loaded parent/subtask relationships, independently of saved-view semantics.
//!
//! Captured S4612673 decodes `parent_record_id`; B7236453/B7241932 index and
//! select its children. S4614487 establishes default position/date ordering.
//! Stock's main grid excludes parented rows (B17874252). This flattened tree is
//! a native presentation, not a port of that grid. Missing/archived/deleted
//! parents and cycles use the explicit preservation policy below.
use super::{Record, RecordsPage};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentRelation<'a> {
    Root,
    Child(&'a str),
    Missing(&'a str),
    Archived(&'a str),
    Deleted(&'a str),
    Cycle(&'a str),
    Invalid,
}

#[derive(Debug)]
pub struct HierarchyRow<'a> {
    pub record: &'a Record,
    pub depth: usize,
    pub relation: ParentRelation<'a>,
    pub has_children: bool,
    /// Exclusive index after all descendants in the returned preorder vector.
    /// A collapsed subtree can be skipped without walking its hidden records.
    pub subtree_end: usize,
}

impl RecordsPage {
    /// Every non-archived, non-deleted loaded record appears exactly once.
    /// Unavailable parent links become visible roots. Every edge within a
    /// parent cycle is cut, rather than inventing an arbitrary ancestor.
    /// Parsing already enforces unique record IDs. Graph work is iterative and
    /// linear after the existing stable default ordering, including deep chains.
    pub fn hierarchy(&self) -> Vec<HierarchyRow<'_>> {
        let all: HashMap<_, _> = self
            .records
            .iter()
            .map(|row| (row.id.as_str(), row))
            .collect();
        let ordered: Vec<_> = self
            .ordered(None)
            .into_iter()
            .filter(|row| !row.archived() && row.raw["is_deleted"] != true)
            .collect();
        let indices: HashMap<_, _> = ordered
            .iter()
            .enumerate()
            .map(|(i, row)| (row.id.as_str(), i))
            .collect();
        let mut parents = Vec::with_capacity(ordered.len());
        let mut relations = Vec::with_capacity(ordered.len());
        for row in &ordered {
            let (parent, relation) = match row.parent_id() {
                Some(id) => match indices.get(id) {
                    Some(index) => (Some(*index), ParentRelation::Child(id)),
                    None => (
                        None,
                        match all.get(id) {
                            Some(parent) if parent.archived() => ParentRelation::Archived(id),
                            Some(parent) if parent.raw["is_deleted"] == true => {
                                ParentRelation::Deleted(id)
                            }
                            _ => ParentRelation::Missing(id),
                        },
                    ),
                },
                None if row
                    .raw
                    .get("parent_record_id")
                    .is_some_and(|value| !value.is_null() && !value.is_string()) =>
                {
                    (None, ParentRelation::Invalid)
                }
                None => (None, ParentRelation::Root),
            };
            parents.push(parent);
            relations.push(relation);
        }

        let mut visited = vec![0u8; ordered.len()];
        let mut path = Vec::new();
        for start in 0..ordered.len() {
            if visited[start] != 0 {
                continue;
            }
            path.clear();
            let mut next = Some(start);
            while let Some(index) = next {
                match visited[index] {
                    0 => {
                        visited[index] = 1;
                        path.push(index);
                        next = parents[index];
                    }
                    1 => {
                        // All older walks were finalized. This visiting node
                        // necessarily belongs to this walk's cyclic suffix.
                        let cycle_start = path
                            .iter()
                            .position(|candidate| *candidate == index)
                            .expect("current parent walk");
                        for &member in &path[cycle_start..] {
                            parents[member] = None;
                            relations[member] = ParentRelation::Cycle(
                                ordered[member].parent_id().expect("cycle parent"),
                            );
                        }
                        break;
                    }
                    _ => break,
                }
            }
            for &index in &path {
                visited[index] = 2;
            }
        }

        let mut children = vec![Vec::new(); ordered.len()];
        let mut roots = Vec::new();
        // Traversal order inherits stable default order for roots and siblings.
        for (index, parent) in parents.into_iter().enumerate() {
            if let Some(parent) = parent {
                children[parent].push(index);
            } else {
                roots.push(index);
            }
        }
        enum Visit {
            Enter(usize, usize),
            Exit(usize),
        }
        let mut stack: Vec<_> = roots
            .into_iter()
            .rev()
            .map(|root| Visit::Enter(root, 0))
            .collect();
        let mut output: Vec<HierarchyRow<'_>> = Vec::with_capacity(ordered.len());
        while let Some(visit) = stack.pop() {
            match visit {
                Visit::Enter(index, depth) => {
                    let position = output.len();
                    output.push(HierarchyRow {
                        record: ordered[index],
                        depth,
                        relation: relations[index],
                        has_children: !children[index].is_empty(),
                        subtree_end: position + 1,
                    });
                    stack.push(Visit::Exit(position));
                    stack.extend(
                        children[index]
                            .iter()
                            .rev()
                            .map(|child| Visit::Enter(*child, depth + 1)),
                    );
                }
                Visit::Exit(position) => {
                    output[position].subtree_end = output.len();
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn row(id: &str, parent: Option<&str>, position: Value) -> Value {
        json!({"id":id,"parent_record_id":parent,"position":position,"date_created":100,"fields":[]})
    }
    fn make_page(rows: Vec<Value>) -> RecordsPage {
        RecordsPage::parse(json!({"ok":true,"records":rows})).unwrap()
    }

    #[test]
    fn roots_and_siblings_keep_default_order_and_descendants_stay_with_parent() {
        let page = make_page(vec![
            row("child-z", Some("parent"), json!("z")),
            row("other", None, json!("b")),
            row("parent", None, json!("a")),
            row("child-a", Some("parent"), json!("a")),
            row("grandchild", Some("child-a"), json!("x")),
        ]);
        let rows = page.hierarchy();
        assert_eq!(
            rows.iter()
                .map(|r| (r.record.id.as_str(), r.depth, r.subtree_end))
                .collect::<Vec<_>>(),
            vec![
                ("parent", 0, 4),
                ("child-a", 1, 3),
                ("grandchild", 2, 3),
                ("child-z", 1, 4),
                ("other", 0, 5)
            ]
        );
        assert!(rows[0].has_children);
        assert!(!rows[2].has_children);
        assert_eq!(rows[1].relation, ParentRelation::Child("parent"));
    }

    #[test]
    fn missing_archived_deleted_and_invalid_parents_preserve_active_children() {
        let mut archived = row("archived", None, json!(0));
        archived["archived"] = json!(true);
        let mut deleted = row("deleted", None, json!(1));
        deleted["is_deleted"] = json!(true);
        let mut invalid = row("invalid", None, json!(5));
        invalid["parent_record_id"] = json!({"id":"unknown"});
        let page = make_page(vec![
            archived,
            deleted,
            row("a", Some("absent"), json!(2)),
            row("b", Some("archived"), json!(3)),
            row("c", Some("deleted"), json!(4)),
            invalid,
            row("grandchild", Some("b"), json!(6)),
        ]);
        let rows = page.hierarchy();
        assert_eq!(rows.len(), 5);
        assert_eq!(
            rows.iter()
                .map(|r| r.record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "grandchild", "c", "invalid"]
        );
        assert_eq!(rows[0].relation, ParentRelation::Missing("absent"));
        assert_eq!(rows[1].relation, ParentRelation::Archived("archived"));
        assert_eq!(rows[3].relation, ParentRelation::Deleted("deleted"));
        assert_eq!(rows[4].relation, ParentRelation::Invalid);
        assert_eq!(rows[2].depth, 1);
    }

    #[test]
    fn cycles_are_roots_with_explicit_notices_and_do_not_swallow_descendants() {
        let page = make_page(vec![
            row("a", Some("b"), json!(0)),
            row("b", Some("c"), json!(1)),
            row("c", Some("a"), json!(2)),
            row("self", Some("self"), json!(3)),
            row("child", Some("b"), json!(4)),
            row("leaf", Some("child"), json!(5)),
        ]);
        let rows = page.hierarchy();
        assert_eq!(
            rows.iter()
                .map(|r| (r.record.id.as_str(), r.depth))
                .collect::<Vec<_>>(),
            vec![
                ("a", 0),
                ("b", 0),
                ("child", 1),
                ("leaf", 2),
                ("c", 0),
                ("self", 0)
            ]
        );
        for id in ["a", "b", "c", "self"] {
            assert!(matches!(
                rows.iter().find(|r| r.record.id == id).unwrap().relation,
                ParentRelation::Cycle(_)
            ));
        }
        assert_eq!(
            rows.iter()
                .map(|r| r.record.id.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len(),
            page.records.len()
        );
    }

    #[test]
    fn deep_parent_chains_and_reversed_input_use_iterative_bounded_work() {
        let count = 20_000;
        let values = (0..count)
            .rev()
            .map(|index| {
                let parent = (index > 0).then(|| format!("row-{}", index - 1));
                row(&format!("row-{index}"), parent.as_deref(), json!(index))
            })
            .collect();
        let page = make_page(values);
        let rows = page.hierarchy();
        assert_eq!(rows.len(), count);
        assert_eq!(rows[count - 1].depth, count - 1);
        assert!(rows.iter().all(|row| row.subtree_end == count));
        assert_eq!(rows[0].record.id, "row-0");
    }

    #[test]
    fn stable_ties_and_incomplete_pages_do_not_invent_saved_view_order() {
        let mut a = row("a", None, json!(1));
        a["view_positions"] = json!({"view":99});
        let mut b = row("b", None, json!(1));
        b["view_positions"] = json!({"view":0});
        let mut page = make_page(vec![a, b, row("orphan", Some("unloaded"), json!(2))]);
        page.next_cursor = Some("more".into());
        assert_eq!(
            page.hierarchy()
                .iter()
                .map(|r| r.record.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "orphan"]
        );
        assert_eq!(
            page.hierarchy()[2].relation,
            ParentRelation::Missing("unloaded")
        );
        assert!(make_page(vec![]).hierarchy().is_empty());
    }
}
