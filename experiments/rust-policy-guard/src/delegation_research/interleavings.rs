//! Test-only interleaving explorer layered on the delegation research table.
//! Not an SMP implementation: every action below is sequential and atomic by
//! assumption. Snapshots and caller identities are forgeable research inputs.
use super::{Budget, Denied, Grant, GrantTable, Rights, Slot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Handle {
    slot: Slot,
    generation: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Observation {
    parent: Handle,
    owner: u32,
    version: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    Grant(Denied),
    StaleHandle,
    StaleObservation,
    CounterExhausted,
}

/// An abstract, serialized coordinator for one finite delegation tree.
/// Neither caller identity nor the test-only retirement precondition is
/// authenticated by a kernel. No DMA, TLB, or in-flight work is tracked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Sequenced {
    table: GrantTable<3>,
    generations: [u8; 3],
    versions: [u8; 3],
}

impl Sequenced {
    fn new() -> Self {
        Self {
            table: GrantTable::root(1, Rights::ALL, Budget::new(8, 8)).unwrap(),
            generations: [1; 3],
            versions: [0; 3],
        }
    }

    const fn root() -> Handle {
        Handle {
            slot: Slot(0),
            generation: 1,
        }
    }

    fn checked_grant(&self, handle: Handle, owner: u32) -> Result<Grant, Error> {
        let generation = self
            .generations
            .get(handle.slot.0)
            .ok_or(Error::StaleHandle)?;
        if *generation != handle.generation {
            return Err(Error::StaleHandle);
        }
        self.table.grant(handle.slot, owner).map_err(Error::Grant)
    }

    fn observe(&self, handle: Handle, owner: u32) -> Result<Observation, Error> {
        self.checked_grant(handle, owner)?;
        Ok(Observation {
            parent: handle,
            owner,
            version: self.versions[handle.slot.0],
        })
    }

    /// Represents commit under one abstract lock. Validate token generation,
    /// caller owner, and observed version against the *current* table first.
    /// This function is NOT safe if its checks/writes are split across threads.
    fn commit(
        &mut self,
        observed: Observation,
        child_owner: u32,
        rights: Rights,
        budget: Budget,
    ) -> Result<Handle, Error> {
        self.checked_grant(observed.parent, observed.owner)?;
        let slot = observed.parent.slot.0;
        if observed.version != self.versions[slot] {
            return Err(Error::StaleObservation);
        }
        let next_version = self.versions[slot]
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let child = self
            .table
            .delegate(
                observed.parent.slot,
                observed.owner,
                child_owner,
                rights,
                budget,
            )
            .map_err(Error::Grant)?;
        self.versions[slot] = next_version;
        Ok(Handle {
            slot: child,
            generation: self.generations[child.0],
        })
    }

    /// Test-fixture retirement assumes all use of this child has already
    /// ended. This is NOT revocation, verified quiescence, or safe reclaim.
    /// Never refund transferred budget; reject counter wrap before mutation.
    fn retire_for_fixture(&mut self, handle: Handle, owner: u32) -> Result<(), Error> {
        self.checked_grant(handle, owner)?;
        if handle.slot.0 == 0 {
            return Err(Error::StaleHandle);
        }
        let next_generation = self.generations[handle.slot.0]
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        let parent_slot = self.table.slots[handle.slot.0]
            .ok_or(Error::StaleHandle)?
            .parent
            .ok_or(Error::StaleHandle)?
            .0;
        let next_parent_version = self.versions[parent_slot]
            .checked_add(1)
            .ok_or(Error::CounterExhausted)?;
        self.table.slots[handle.slot.0] = None;
        self.generations[handle.slot.0] = next_generation;
        self.versions[parent_slot] = next_parent_version;
        Ok(())
    }
}

#[test]
fn stale_handles_are_rejected_after_test_only_slot_reuse() {
    let mut model = Sequenced::new();
    let old = model
        .commit(
            model.observe(Sequenced::root(), 1).unwrap(),
            2,
            Rights::CPU,
            Budget::new(2, 0),
        )
        .unwrap();
    assert_eq!(model.checked_grant(old, 2).unwrap().owner, 2);
    model.retire_for_fixture(old, 2).unwrap();

    let new = model
        .commit(
            model.observe(Sequenced::root(), 1).unwrap(),
            3,
            Rights::CPU,
            Budget::new(2, 0),
        )
        .unwrap();
    assert_eq!(old.slot, new.slot);
    assert_ne!(old.generation, new.generation);
    assert_eq!(model.checked_grant(old, 2), Err(Error::StaleHandle));
    assert_eq!(model.checked_grant(new, 3).unwrap().owner, 3);
    // Retirement does not refill the parent's already-transferred budget.
    assert_eq!(
        model.table.grant(Slot(0), 1).unwrap().remaining,
        Budget::new(4, 8)
    );
}

#[test]
fn stale_observation_is_rejected_after_child_slot_changes() {
    let mut model = Sequenced::new();
    let snapshot = model.observe(Sequenced::root(), 1).unwrap();
    let child = model
        .commit(snapshot, 2, Rights::CPU, Budget::new(2, 0))
        .unwrap();
    model.retire_for_fixture(child, 2).unwrap();
    let unchanged = model.clone();
    assert_eq!(
        model.commit(snapshot, 3, Rights::CPU, Budget::new(1, 0)),
        Err(Error::StaleObservation)
    );
    assert_eq!(model, unchanged);
}

#[test]
fn all_two_request_interleavings_serialize_budget_debits() {
    // Both requests inspect the same initial state. Explore both commit
    // orders and a fresh retry for each pair of positive CPU/page requests.
    for left in 1..=8u16 {
        for right in 1..=8u16 {
            for reverse in [false, true] {
                let mut model = Sequenced::new();
                let one = model.observe(Sequenced::root(), 1).unwrap();
                let two = model.observe(Sequenced::root(), 1).unwrap();
                let (first, second) = if reverse {
                    (right, left)
                } else {
                    (left, right)
                };
                model
                    .commit(
                        one,
                        2,
                        Rights::CPU | Rights::MEMORY,
                        Budget::new(first, first),
                    )
                    .unwrap();
                let after_first = model.clone();
                assert_eq!(
                    model.commit(
                        two,
                        3,
                        Rights::CPU | Rights::MEMORY,
                        Budget::new(second, second)
                    ),
                    Err(Error::StaleObservation)
                );
                assert_eq!(model, after_first);
                let retry = model.commit(
                    model.observe(Sequenced::root(), 1).unwrap(),
                    3,
                    Rights::CPU | Rights::MEMORY,
                    Budget::new(second, second),
                );
                assert_eq!(retry.is_ok(), first + second <= 8);
                let left_balance = model.table.grant(Slot(0), 1).unwrap().remaining;
                let delegated = if retry.is_ok() {
                    first + second
                } else {
                    first
                };
                assert_eq!(left_balance, Budget::new(8 - delegated, 8 - delegated));
            }
        }
    }
}

#[test]
fn exhausted_counters_and_forged_owners_fail_without_mutation() {
    let mut model = Sequenced::new();
    let root = Sequenced::root();
    let snapshot = model.observe(root, 1).unwrap();
    assert_eq!(model.observe(root, 2), Err(Error::Grant(Denied::WrongOwner)));
    model.versions[0] = u8::MAX;
    let before = model.clone();
    assert_eq!(
        model.commit(
            Observation {
                version: u8::MAX,
                ..snapshot
            },
            2,
            Rights::CPU,
            Budget::new(1, 0)
        ),
        Err(Error::CounterExhausted)
    );
    assert_eq!(model, before);

    let mut other = Sequenced::new();
    let child = other
        .commit(snapshot, 2, Rights::CPU, Budget::new(1, 0))
        .unwrap();
    other.generations[child.slot.0] = u8::MAX;
    let matching = Handle {
        generation: u8::MAX,
        ..child
    };
    let before = other.clone();
    assert_eq!(
        other.retire_for_fixture(matching, 2),
        Err(Error::CounterExhausted)
    );
    assert_eq!(other, before);
}
