//! M06d: bounded, single-writer reservation bookkeeping for physical ranges.
//! This module does not authenticate callers or establish mapping authority.
use crate::memory::{FramePool, ReservationError, MIN_PHYSICAL_ADDRESS, PAGE_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameReservation {
    pub start: u64,
    pub pages: u64,
    pub owner: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    ZeroPages,
    InvalidOwner,
    InvalidStart,
    AddressOverflow,
    Overlap,
    CapacityExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    Pool(ReservationError),
    Registry(RegistryError),
    InvariantViolation,
}

/// A single-writer, fixed-capacity registry. Reservations never expire, and
/// there is deliberately no release path until safe reclamation is specified.
pub struct FrameRegistry<const N: usize> {
    entries: [Option<FrameReservation>; N],
    count: usize,
}

impl<const N: usize> FrameRegistry<N> {
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            count: 0,
        }
    }

    #[allow(dead_code)]
    pub const fn count(&self) -> usize {
        self.count
    }

    fn validate(
        &self,
        start: u64,
        pages: u64,
        owner: u32,
    ) -> Result<FrameReservation, RegistryError> {
        if owner == 0 {
            return Err(RegistryError::InvalidOwner);
        }
        if pages == 0 {
            return Err(RegistryError::ZeroPages);
        }
        if start < MIN_PHYSICAL_ADDRESS || !start.is_multiple_of(PAGE_BYTES) {
            return Err(RegistryError::InvalidStart);
        }
        let bytes = pages
            .checked_mul(PAGE_BYTES)
            .ok_or(RegistryError::AddressOverflow)?;
        let end = start
            .checked_add(bytes)
            .ok_or(RegistryError::AddressOverflow)?;

        for existing in self.entries[..self.count].iter().flatten() {
            // Every stored reservation passed validate(), including both checked
            // arithmetic operations. Recheck here so future insertion paths
            // cannot silently turn a corrupted invariant into an overflow.
            let existing_bytes = existing
                .pages
                .checked_mul(PAGE_BYTES)
                .ok_or(RegistryError::AddressOverflow)?;
            let existing_end = existing
                .start
                .checked_add(existing_bytes)
                .ok_or(RegistryError::AddressOverflow)?;
            if start < existing_end && existing.start < end {
                return Err(RegistryError::Overlap);
            }
        }
        if self.count == N {
            return Err(RegistryError::CapacityExhausted);
        }

        Ok(FrameReservation {
            start,
            pages,
            owner,
        })
    }

    fn insert_validated(&mut self, reservation: FrameReservation) {
        self.entries[self.count] = Some(reservation);
        self.count += 1;
    }

    /// Record a trusted physical run without touching actual physical memory.
    /// Any rejection leaves all registry entries unchanged.
    ///
    /// Caller must independently establish that the range is USABLE, uniquely
    /// sourced, and not accessible through conflicting CPU or device mappings.
    #[allow(dead_code)]
    pub fn reserve(
        &mut self,
        start: u64,
        pages: u64,
        owner: u32,
    ) -> Result<FrameReservation, RegistryError> {
        let reservation = self.validate(start, pages, owner)?;
        self.insert_validated(reservation);
        Ok(reservation)
    }

    #[allow(dead_code)]
    pub fn entry(&self, index: usize) -> Option<FrameReservation> {
        self.entries.get(index).copied().flatten()
    }
}

/// Single BSP-owned allocation authority: one pool + one registry.
pub struct TransactionalFrameAllocator<const N: usize> {
    pool: FramePool,
    registry: FrameRegistry<N>,
}

impl<const N: usize> TransactionalFrameAllocator<N> {
    pub fn new(pool: FramePool) -> Self {
        Self {
            pool,
            registry: FrameRegistry::new(),
        }
    }

    /// Initialize the allocator with the boot diagnostic reservation.
    pub fn with_boot_reservation(pool: FramePool) -> Result<Self, TransactionError> {
        let mut allocator = Self::new(pool);
        allocator.reserve(2, 1)?;
        Ok(allocator)
    }

    pub fn reservation(&self, index: usize) -> Option<FrameReservation> {
        self.registry.entry(index)
    }

    /// Reserve contiguous pages and record ownership exactly once.
    /// Rejections leave both pool and registry unchanged.
    pub fn reserve(
        &mut self,
        pages: u64,
        owner: u32,
    ) -> Result<FrameReservation, TransactionError> {
        let run = self
            .pool
            .preview_contiguous(pages)
            .map_err(TransactionError::Pool)?;
        let start = run
            .address_at(0)
            .ok_or(TransactionError::InvariantViolation)?;
        let reservation = self
            .registry
            .validate(start, pages, owner)
            .map_err(TransactionError::Registry)?;
        self.pool
            .reserve_contiguous(pages)
            .map_err(TransactionError::Pool)?;
        self.registry.insert_validated(reservation);
        Ok(reservation)
    }

    pub const fn remaining_pages(&self) -> u64 {
        self.pool.remaining()
    }

    #[cfg(feature = "boot-ownership-smoke")]
    pub fn pool_mut(&mut self) -> &mut FramePool {
        &mut self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{choose_pool, Region, USABLE_KIND};

    fn pool(pages: u64) -> FramePool {
        choose_pool(
            &[Region {
                base: MIN_PHYSICAL_ADDRESS,
                length: PAGE_BYTES * pages,
                kind: USABLE_KIND,
            }],
            1,
        )
        .unwrap()
    }

    #[test]
    fn adjacent_ranges_and_distinct_owners_are_allowed() {
        let mut registry = FrameRegistry::<2>::new();
        assert_eq!(registry.reserve(0x100000, 2, 1).unwrap().start, 0x100000);
        assert_eq!(registry.reserve(0x102000, 1, 2).unwrap().owner, 2);
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn overlaps_are_rejected_across_owners_and_same_owner() {
        let mut registry = FrameRegistry::<4>::new();
        registry.reserve(0x101000, 4, 1).unwrap();
        for (start, pages, owner) in [
            (0x100000, 2, 2),
            (0x102000, 1, 1),
            (0x104000, 2, 2),
            (0x101000, 4, 1),
        ] {
            assert_eq!(
                registry.reserve(start, pages, owner),
                Err(RegistryError::Overlap)
            );
        }
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn invalid_inputs_never_mutate_state() {
        let mut registry = FrameRegistry::<2>::new();
        for (start, pages, owner, expected) in [
            (0x100000, 1, 0, RegistryError::InvalidOwner),
            (0x100000, 0, 1, RegistryError::ZeroPages),
            (0x100001, 1, 1, RegistryError::InvalidStart),
            (0x0, 1, 1, RegistryError::InvalidStart),
            (0x100000, u64::MAX, 1, RegistryError::AddressOverflow),
            (0xffff_ffff_ffff_f000, 2, 1, RegistryError::AddressOverflow),
        ] {
            assert_eq!(registry.reserve(start, pages, owner), Err(expected));
        }
        assert_eq!(registry.count(), 0);
        assert_eq!(registry.entry(0), None);
    }

    #[test]
    fn near_address_limit_valid_reservation_remains_disjoint() {
        let mut registry = FrameRegistry::<2>::new();
        let last_page = u64::MAX - (PAGE_BYTES - 1);
        let reservation = registry.reserve(last_page - PAGE_BYTES, 1, 1).unwrap();
        assert_eq!(reservation.start, last_page - PAGE_BYTES);
        assert_eq!(
            registry.reserve(last_page - PAGE_BYTES, 1, 2),
            Err(RegistryError::Overlap)
        );
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.entry(0), Some(reservation));
    }

    #[test]
    fn invalid_existing_endpoint_fails_closed_without_mutation() {
        let mut registry = FrameRegistry::<2>::new();
        // Simulate an invalid internal record to test the defensive invariant.
        registry.entries[0] = Some(FrameReservation {
            start: u64::MAX - (PAGE_BYTES - 1),
            pages: 2,
            owner: 1,
        });
        registry.count = 1;
        assert_eq!(
            registry.reserve(MIN_PHYSICAL_ADDRESS, 1, 2),
            Err(RegistryError::AddressOverflow)
        );
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.entry(1), None);
    }

    #[test]
    fn capacity_exhaustion_does_not_consume_a_range() {
        let mut registry = FrameRegistry::<1>::new();
        registry.reserve(0x100000, 1, 1).unwrap();
        assert_eq!(
            registry.reserve(0x200000, 1, 2),
            Err(RegistryError::CapacityExhausted)
        );
        assert_eq!(registry.entry(0).unwrap().start, 0x100000);
        assert_eq!(registry.count(), 1);
    }

    #[test]
    fn zero_capacity_fails_closed() {
        let mut registry = FrameRegistry::<0>::new();
        assert_eq!(
            registry.reserve(0x100000, 1, 1),
            Err(RegistryError::CapacityExhausted)
        );
    }

    #[test]
    fn reservation_records_persist_without_reuse() {
        let mut registry = FrameRegistry::<2>::new();
        let snapshot = registry.reserve(0x100000, 1, 7).unwrap();
        assert_eq!(snapshot, registry.entry(0).unwrap());
        assert_eq!(
            registry.reserve(0x100000, 1, 8),
            Err(RegistryError::Overlap)
        );
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.entry(2), None);
    }

    #[test]
    fn boot_reservation_survives_initialization_and_remains_disjoint() {
        fn initialize() -> TransactionalFrameAllocator<2> {
            TransactionalFrameAllocator::with_boot_reservation(pool(4)).unwrap()
        }

        let mut allocator = initialize();
        let initial = allocator.reservation(0).unwrap();
        assert_eq!(
            (initial.start, initial.pages, initial.owner),
            (0x100000, 2, 1)
        );
        let next = allocator.reserve(1, 2).unwrap();
        assert_eq!(next.start, initial.start + 2 * PAGE_BYTES);
        assert_eq!(allocator.reservation(0), Some(initial));
        assert_eq!(allocator.reservation(1), Some(next));
    }

    #[test]
    fn transactional_capacity_rejection_leaves_pool_and_registry_unchanged() {
        let mut allocator = TransactionalFrameAllocator::<1>::new(pool(4));
        let first = allocator.reserve(1, 1).unwrap();
        assert_eq!(first.start, 0x100000);

        let remaining_before = allocator.remaining_pages();
        let count_before = allocator.registry.count();
        assert_eq!(
            allocator.reserve(1, 2),
            Err(TransactionError::Registry(RegistryError::CapacityExhausted))
        );
        assert_eq!(allocator.registry.count(), count_before);
        assert_eq!(allocator.remaining_pages(), remaining_before);
        assert_eq!(allocator.registry.entry(0), Some(first));
    }

    #[test]
    fn transactional_overlap_rejection_does_not_advance_pool() {
        let mut allocator = TransactionalFrameAllocator::<2>::new(pool(4));
        allocator.registry.reserve(0x100000, 1, 9).unwrap();

        let remaining_before = allocator.remaining_pages();
        let count_before = allocator.registry.count();
        assert_eq!(
            allocator.reserve(1, 1),
            Err(TransactionError::Registry(RegistryError::Overlap))
        );
        assert_eq!(allocator.registry.count(), count_before);
        assert_eq!(allocator.remaining_pages(), remaining_before);
        assert_eq!(allocator.registry.entry(0).unwrap().owner, 9);
    }
}
