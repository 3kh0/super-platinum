use super::*;
use std::{cell::RefCell, rc::Rc};

fn owner(account: &str, epoch: u64) -> Option<Owner> {
    Some(Owner {
        account: account.into(),
        epoch,
    })
}

#[test]
fn number_dot_and_clear_are_distinct_and_repeated_values_do_not_write() {
    let writes = Rc::new(RefCell::new(Vec::new()));
    let output = writes.clone();
    let mut controller = Controller::with_writer(move |label| output.borrow_mut().push(label));
    controller.update(owner("a", 1), DockBadge::Count(42));
    controller.update(owner("a", 1), DockBadge::Count(42));
    controller.update(owner("a", 1), DockBadge::Dot);
    controller.update(owner("a", 1), DockBadge::Dot);
    controller.update(owner("a", 1), DockBadge::Empty);
    controller.update(owner("a", 1), DockBadge::Count(0));
    drop(controller);
    assert_eq!(
        *writes.borrow(),
        vec![None, Some("42".into()), Some("•".into()), None]
    );
}

#[test]
fn account_epoch_retirement_and_drop_clear_exactly_once() {
    let writes = Rc::new(RefCell::new(Vec::new()));
    let output = writes.clone();
    let mut controller = Controller::with_writer(move |label| output.borrow_mut().push(label));
    controller.update(owner("a", 1), DockBadge::Count(7));
    controller.update(owner("a", 2), DockBadge::Count(7));
    controller.update(owner("b", 2), DockBadge::Count(7));
    controller.update(None, DockBadge::Count(99));
    controller.update(None, DockBadge::Dot);
    assert_eq!(
        *writes.borrow(),
        vec![
            None,
            Some("7".into()),
            None,
            Some("7".into()),
            None,
            Some("7".into()),
            None
        ]
    );
    controller.update(owner("b", 3), DockBadge::Dot);
    drop(controller);
    assert_eq!(writes.borrow().last(), Some(&None));
    assert_eq!(writes.borrow().len(), 9);
}
