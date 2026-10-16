use super::*;

#[test]
fn reads_keep_stamp_but_equal_value_mutation_and_aba_invalidate() {
    let owner = Tracked::new(String::from("original"));
    let (stamp, value) = owner.observe(Clone::clone).unwrap();
    assert_eq!(value, "original");
    assert_eq!(&*owner.lock().unwrap(), "original");
    owner.validate(&stamp).unwrap();
    *owner.lock().unwrap() = String::from("original");
    assert_eq!(owner.validate(&stamp), Err(Error::Changed));
    let (stamp, ()) = owner.observe(|_| ()).unwrap();
    *owner.lock().unwrap() = String::from("newer");
    *owner.lock().unwrap() = String::from("original");
    assert_eq!(owner.validate(&stamp), Err(Error::Changed));
}

#[test]
fn replacement_allocation_cannot_validate_identical_payload_and_generation() {
    let first = Tracked::new(7);
    let second = Tracked::new(7);
    let (stamp, ()) = first.observe(|_| ()).unwrap();
    assert_eq!(second.validate(&stamp), Err(Error::Replaced));
}

#[test]
fn try_lock_preserves_exclusion_and_mutable_access_tracking() {
    let owner = Tracked::new(1);
    let (stamp, ()) = owner.observe(|_| ()).unwrap();
    let guard = owner.lock().unwrap();
    assert!(matches!(owner.try_lock(), Err(TryLockError::WouldBlock)));
    drop(guard);
    assert_eq!(*owner.try_lock().unwrap(), 1);
    owner.validate(&stamp).unwrap();
    *owner.try_lock().unwrap() = 2;
    assert_eq!(owner.validate(&stamp), Err(Error::Changed));
}

#[test]
fn exhaustion_keeps_actual_mutation_but_permanently_refuses_observation() {
    let owner = Tracked::new(0);
    owner.inner.lock().unwrap().generation = Some(u64::MAX);
    let (stamp, ()) = owner.observe(|_| ()).unwrap();
    *owner.lock().unwrap() = 1;
    assert_eq!(*owner.lock().unwrap(), 1);
    assert_eq!(owner.validate(&stamp), Err(Error::Unknown));
    *owner.lock().unwrap() = 2;
    assert!(matches!(owner.observe(|_| ()), Err(Error::Unknown)));
    assert_eq!(*owner.lock().unwrap(), 2);
}

#[test]
fn poison_is_unknown_and_keeps_the_original_mutated_payload() {
    let owner = Tracked::new(String::from("before"));
    let (stamp, ()) = owner.observe(|_| ()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut value = owner.lock().unwrap();
        *value = String::from("retained");
        panic!("deliberate test poison");
    }));
    assert!(result.is_err());
    assert_eq!(owner.validate(&stamp), Err(Error::Unknown));
    assert!(matches!(owner.observe(|_| ()), Err(Error::Unknown)));
    assert_eq!(&**owner.lock().err().unwrap().get_ref(), "retained");
}
