//! Host-test-only capability delegation model.
//!
//! Not kernel authority: caller IDs are supplied by tests, slots are not
//! protected, and no CPU time or physical memory is actually allocated.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rights(u8);

impl Rights {
    const CPU: Self = Self(0b001);
    const MEMORY: Self = Self(0b010);
    const DELEGATE: Self = Self(0b100);
    const ALL: Self = Self(0b111);

    const fn contains(self, requested: Self) -> bool {
        (self.0 & requested.0) == requested.0
    }

    const fn valid(self) -> bool {
        self.0 != 0 && (self.0 & !Self::ALL.0) == 0
    }

    const fn subset_of(self, parent: Self) -> bool {
        (self.0 & !parent.0) == 0
    }
}

impl core::ops::BitOr for Rights {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Budget {
    cpu_ticks: u16,
    pages: u16,
}

impl Budget {
    const fn new(cpu_ticks: u16, pages: u16) -> Self {
        Self { cpu_ticks, pages }
    }

    const fn empty(self) -> bool {
        self.cpu_ticks == 0 && self.pages == 0
    }

    const fn fits(self, request: Self) -> bool {
        self.cpu_ticks >= request.cpu_ticks && self.pages >= request.pages
    }

    fn debit(self, request: Self) -> Self {
        // All callers first check fits(), so both subtractions are in range.
        Self::new(
            self.cpu_ticks - request.cpu_ticks,
            self.pages - request.pages,
        )
    }

    fn allowed_by(self, rights: Rights) -> bool {
        (self.cpu_ticks == 0 || rights.contains(Rights::CPU))
            && (self.pages == 0 || rights.contains(Rights::MEMORY))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grant {
    owner: u32,
    rights: Rights,
    remaining: Budget,
    parent: Option<Slot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Denied {
    InvalidOwner,
    InvalidRights,
    EmptyBudget,
    MissingSlot,
    WrongOwner,
    MissingDelegateRight,
    RightsAmplification,
    BudgetRightsMismatch,
    BudgetExhausted,
    TableFull,
}

/// Simulated kernel-owned table. There is no revocation, handle recycling,
/// synchronization, trusted caller identity or hardware resource enforcement.
#[derive(Clone, Debug, PartialEq, Eq)]
struct GrantTable<const N: usize> {
    slots: [Option<Grant>; N],
}

impl<const N: usize> GrantTable<N> {
    fn root(owner: u32, rights: Rights, budget: Budget) -> Result<Self, Denied> {
        if owner == 0 {
            return Err(Denied::InvalidOwner);
        }
        if !rights.valid() {
            return Err(Denied::InvalidRights);
        }
        if budget.empty() {
            return Err(Denied::EmptyBudget);
        }
        if !budget.allowed_by(rights) {
            return Err(Denied::BudgetRightsMismatch);
        }
        if N == 0 {
            return Err(Denied::TableFull);
        }

        let mut slots = [None; N];
        slots[0] = Some(Grant {
            owner,
            rights,
            remaining: budget,
            parent: None,
        });
        Ok(Self { slots })
    }

    fn grant(&self, slot: Slot, caller_owner: u32) -> Result<Grant, Denied> {
        let entry = self
            .slots
            .get(slot.0)
            .and_then(|entry| *entry)
            .ok_or(Denied::MissingSlot)?;
        if entry.owner != caller_owner {
            return Err(Denied::WrongOwner);
        }
        Ok(entry)
    }

    /// Move *delegable* budget into a child record; do not mint new budget.
    /// All rejection conditions precede both table writes.
    fn delegate(
        &mut self,
        parent: Slot,
        caller_owner: u32,
        child_owner: u32,
        child_rights: Rights,
        budget: Budget,
    ) -> Result<Slot, Denied> {
        let current = self.grant(parent, caller_owner)?;
        if child_owner == 0 {
            return Err(Denied::InvalidOwner);
        }
        if !current.rights.contains(Rights::DELEGATE) {
            return Err(Denied::MissingDelegateRight);
        }
        if !child_rights.valid() {
            return Err(Denied::InvalidRights);
        }
        if !child_rights.subset_of(current.rights) {
            return Err(Denied::RightsAmplification);
        }
        if budget.empty() {
            return Err(Denied::EmptyBudget);
        }
        if !budget.allowed_by(child_rights) {
            return Err(Denied::BudgetRightsMismatch);
        }
        if !current.remaining.fits(budget) {
            return Err(Denied::BudgetExhausted);
        }
        let free = self
            .slots
            .iter()
            .position(Option::is_none)
            .ok_or(Denied::TableFull)?;

        // The parent was checked above; no mutation occurs on rejection.
        self.slots[parent.0] = Some(Grant {
            remaining: current.remaining.debit(budget),
            ..current
        });
        self.slots[free] = Some(Grant {
            owner: child_owner,
            rights: child_rights,
            remaining: budget,
            parent: Some(parent),
        });
        Ok(Slot(free))
    }

    /// Charge a modeled local reservation against the same remaining budget
    /// used by delegation. It does not execute or reserve physical resources.
    fn charge(&mut self, slot: Slot, caller_owner: u32, budget: Budget) -> Result<(), Denied> {
        let current = self.grant(slot, caller_owner)?;
        if budget.empty() {
            return Err(Denied::EmptyBudget);
        }
        if !budget.allowed_by(current.rights) {
            return Err(Denied::BudgetRightsMismatch);
        }
        if !current.remaining.fits(budget) {
            return Err(Denied::BudgetExhausted);
        }
        self.slots[slot.0] = Some(Grant {
            remaining: current.remaining.debit(budget),
            ..current
        });
        Ok(())
    }
}

#[test]
fn delegated_rights_and_budgets_are_attenuated() {
    let mut table = GrantTable::<4>::root(1, Rights::ALL, Budget::new(100, 10)).unwrap();
    let child = table
        .delegate(
            Slot(0),
            1,
            2,
            Rights::CPU | Rights::DELEGATE,
            Budget::new(30, 0),
        )
        .unwrap();
    let grandchild = table
        .delegate(child, 2, 3, Rights::CPU, Budget::new(15, 0))
        .unwrap();
    assert_eq!(table.grant(grandchild, 3).unwrap().parent, Some(child));
    assert_eq!(table.grant(child, 2).unwrap().remaining, Budget::new(15, 0));
    assert_eq!(
        table.grant(Slot(0), 1).unwrap().remaining,
        Budget::new(70, 10)
    );
    assert_eq!(
        table.delegate(child, 2, 4, Rights::MEMORY, Budget::new(0, 1)),
        Err(Denied::RightsAmplification)
    );
    assert_eq!(
        table.delegate(grandchild, 3, 4, Rights::CPU, Budget::new(1, 0)),
        Err(Denied::MissingDelegateRight)
    );
}

#[test]
fn siblings_and_parent_charge_share_one_finite_budget() {
    let mut table = GrantTable::<5>::root(1, Rights::ALL, Budget::new(100, 10)).unwrap();
    table
        .delegate(
            Slot(0),
            1,
            2,
            Rights::CPU | Rights::MEMORY,
            Budget::new(40, 3),
        )
        .unwrap();
    table
        .delegate(
            Slot(0),
            1,
            3,
            Rights::CPU | Rights::MEMORY,
            Budget::new(50, 5),
        )
        .unwrap();
    table.charge(Slot(0), 1, Budget::new(10, 2)).unwrap();
    let before = table.clone();
    assert_eq!(
        table.delegate(Slot(0), 1, 4, Rights::CPU, Budget::new(1, 0)),
        Err(Denied::BudgetExhausted)
    );
    assert_eq!(table, before);
}

#[test]
fn invalid_owner_and_rights_cannot_change_table() {
    let mut table = GrantTable::<3>::root(1, Rights::ALL, Budget::new(20, 4)).unwrap();
    let before = table.clone();
    assert_eq!(
        table.delegate(Slot(0), 99, 2, Rights::CPU, Budget::new(1, 0)),
        Err(Denied::WrongOwner)
    );
    assert_eq!(
        table.delegate(Slot(0), 1, 0, Rights::CPU, Budget::new(1, 0)),
        Err(Denied::InvalidOwner)
    );
    assert_eq!(
        table.delegate(Slot(0), 1, 2, Rights(0b1000), Budget::new(1, 0)),
        Err(Denied::InvalidRights)
    );
    assert_eq!(
        table.delegate(Slot(0), 1, 2, Rights::CPU, Budget::new(0, 1)),
        Err(Denied::BudgetRightsMismatch)
    );
    assert_eq!(
        table.delegate(Slot(0), 1, 2, Rights::CPU, Budget::new(0, 0)),
        Err(Denied::EmptyBudget)
    );
    assert_eq!(table, before);
}

#[test]
fn full_table_and_missing_slots_fail_without_partial_debit() {
    let mut table = GrantTable::<2>::root(1, Rights::ALL, Budget::new(10, 2)).unwrap();
    assert_eq!(table.grant(Slot(9), 1), Err(Denied::MissingSlot));
    table
        .delegate(Slot(0), 1, 2, Rights::CPU, Budget::new(1, 0))
        .unwrap();
    let before = table.clone();
    assert_eq!(
        table.delegate(Slot(0), 1, 3, Rights::CPU, Budget::new(1, 0)),
        Err(Denied::TableFull)
    );
    assert_eq!(table, before);
}

#[test]
fn charging_child_cannot_double_spend_or_bypass_rights() {
    let mut table = GrantTable::<2>::root(1, Rights::ALL, Budget::new(50, 5)).unwrap();
    let child = table
        .delegate(Slot(0), 1, 2, Rights::MEMORY, Budget::new(0, 3))
        .unwrap();
    table.charge(child, 2, Budget::new(0, 2)).unwrap();
    let before = table.clone();
    assert_eq!(
        table.charge(child, 2, Budget::new(0, 2)),
        Err(Denied::BudgetExhausted)
    );
    assert_eq!(
        table.charge(child, 2, Budget::new(1, 0)),
        Err(Denied::BudgetRightsMismatch)
    );
    assert_eq!(table, before);
}

#[test]
fn root_input_and_zero_capacity_are_rejected() {
    assert_eq!(
        GrantTable::<0>::root(1, Rights::ALL, Budget::new(1, 1)),
        Err(Denied::TableFull)
    );
    assert_eq!(
        GrantTable::<2>::root(0, Rights::ALL, Budget::new(1, 1)),
        Err(Denied::InvalidOwner)
    );
    assert_eq!(
        GrantTable::<2>::root(1, Rights(0), Budget::new(1, 1)),
        Err(Denied::InvalidRights)
    );
    assert_eq!(
        GrantTable::<2>::root(1, Rights::ALL, Budget::new(0, 0)),
        Err(Denied::EmptyBudget)
    );
}

#[test]
fn exhaustive_small_sibling_sums_never_exceed_root_budget() {
    for first_cpu in 0..=4u16 {
        for first_pages in 0..=4u16 {
            if first_cpu == 0 && first_pages == 0 {
                continue;
            }
            for second_cpu in 0..=4u16 {
                for second_pages in 0..=4u16 {
                    if second_cpu == 0 && second_pages == 0 {
                        continue;
                    }
                    let mut table =
                        GrantTable::<3>::root(1, Rights::ALL, Budget::new(4, 4)).unwrap();
                    let rights = Rights::CPU | Rights::MEMORY;
                    table
                        .delegate(Slot(0), 1, 2, rights, Budget::new(first_cpu, first_pages))
                        .unwrap();
                    let second = table.delegate(
                        Slot(0),
                        1,
                        3,
                        rights,
                        Budget::new(second_cpu, second_pages),
                    );
                    let fits = first_cpu + second_cpu <= 4 && first_pages + second_pages <= 4;
                    assert_eq!(second.is_ok(), fits);
                    if !fits {
                        assert_eq!(second, Err(Denied::BudgetExhausted));
                        assert_eq!(
                            table.grant(Slot(0), 1).unwrap().remaining,
                            Budget::new(4 - first_cpu, 4 - first_pages)
                        );
                    }
                }
            }
        }
    }
}
