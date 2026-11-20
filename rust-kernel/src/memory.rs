//! Fixed-capacity, no_std frame accounting; does not dereference physical memory.
pub const PAGE_BYTES: u64 = 4096;
pub const MIN_PHYSICAL_ADDRESS: u64 = 0x0010_0000;
pub const USABLE_KIND: u64 = 0;
pub const MAX_BOOT_REGIONS: usize = 128;
pub const REQUIRED_BOOT_PAGES: u64 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}

impl Region {
    pub const EMPTY: Self = Self {
        base: 0,
        length: 0,
        kind: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapError {
    EmptyMap,
    ZeroLength,
    AddressOverflow,
    OverlappingRegions,
    InvalidMinimum,
    NoUsableSpan,
}

/// Errors for a bounded contiguous reservation from one monotonic pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReservationError {
    ZeroPages,
    AddressOverflow,
    InsufficientFrames,
}

/// Immutable description of physical frame numbers, never a mapped pointer
/// or transferable authority. Dropping this value does not free the frames.
#[derive(Debug, PartialEq, Eq)]
pub struct FrameRun {
    start: u64,
    pages: u64,
}

impl FrameRun {
    /// Get a physical frame number within the reserved run, not a virtual address.
    pub fn address_at(&self, index: u64) -> Option<u64> {
        if index >= self.pages {
            return None;
        }
        let offset = index.checked_mul(PAGE_BYTES)?;
        self.start.checked_add(offset)
    }
}

/// Monotonic bookkeeping for one BSP; not hardware ownership or a page table.
/// The same physical range must never be supplied to a second FramePool.
#[derive(Debug, PartialEq, Eq)]
pub struct FramePool {
    next: u64,
    end: u64,
}

impl FramePool {
    fn checked_run(next: u64, end: u64, pages: u64) -> Result<FrameRun, ReservationError> {
        if pages == 0 {
            return Err(ReservationError::ZeroPages);
        }
        let bytes = pages
            .checked_mul(PAGE_BYTES)
            .ok_or(ReservationError::AddressOverflow)?;
        let reservation_end = next
            .checked_add(bytes)
            .ok_or(ReservationError::AddressOverflow)?;
        if reservation_end > end {
            return Err(ReservationError::InsufficientFrames);
        }
        Ok(FrameRun { start: next, pages })
    }

    /// Validate a nonempty consecutive reservation without advancing the pool.
    pub fn preview_contiguous(&self, pages: u64) -> Result<FrameRun, ReservationError> {
        Self::checked_run(self.next, self.end, pages)
    }

    /// Atomically reserve a nonempty, consecutive run of frame numbers.
    /// Validation errors and exhaustion leave the pool unchanged.
    pub fn reserve_contiguous(&mut self, pages: u64) -> Result<FrameRun, ReservationError> {
        let run = Self::checked_run(self.next, self.end, pages)?;
        let bytes = pages
            .checked_mul(PAGE_BYTES)
            .ok_or(ReservationError::AddressOverflow)?;
        self.next = self
            .next
            .checked_add(bytes)
            .ok_or(ReservationError::AddressOverflow)?;
        Ok(run)
    }

    // Used by M05's optional ownership smoke and the host regression tests.
    // Normal M06a boot diagnostics use reserve_contiguous directly.
    #[cfg(any(test, feature = "boot-ownership-smoke"))]
    pub fn claim(&mut self) -> Option<u64> {
        self.reserve_contiguous(1).ok()?.address_at(0)
    }

    pub const fn remaining(&self) -> u64 {
        (self.end - self.next) / PAGE_BYTES
    }
}

fn page_align_up(address: u64) -> Option<u64> {
    address
        .checked_add(PAGE_BYTES - 1)
        .map(|value| value & !(PAGE_BYTES - 1))
}

// One production permit for this kernel image. Consuming it is irreversible,
// including when memory-map validation later fails.
static BOOT_POOL_ISSUED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub struct BootPoolPermit {
    _private: (),
}

pub fn claim_boot_pool_permit() -> Option<BootPoolPermit> {
    use core::sync::atomic::Ordering;
    BOOT_POOL_ISSUED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()
        .map(|_| BootPoolPermit { _private: () })
}

/// The permit is consumed even when validation fails. This only controls pool
/// construction through this module; it does not establish hardware ownership.
pub fn select_boot_pool(
    _permit: BootPoolPermit,
    regions: &[Region],
    min_pages: u64,
) -> Result<FramePool, MapError> {
    select_pool(regions, min_pages)
}

// Pure selector exposed to host tests only; it does not represent authority.
#[cfg(test)]
pub fn choose_pool(regions: &[Region], min_pages: u64) -> Result<FramePool, MapError> {
    select_pool(regions, min_pages)
}

/// Check every boot-map entry for invalid ranges and reject any overlap
/// involving usable memory before issuing a frame. Limine permits overlap
/// between non-usable entries; those ranges are never allocated here.
fn select_pool(regions: &[Region], min_pages: u64) -> Result<FramePool, MapError> {
    if regions.is_empty() {
        return Err(MapError::EmptyMap);
    }
    if min_pages == 0 {
        return Err(MapError::InvalidMinimum);
    }

    let mut choice: Option<FramePool> = None;
    for (index, region) in regions.iter().enumerate() {
        if region.length == 0 {
            return Err(MapError::ZeroLength);
        }
        let end = region
            .base
            .checked_add(region.length)
            .ok_or(MapError::AddressOverflow)?;
        for earlier in &regions[..index] {
            let earlier_end = earlier
                .base
                .checked_add(earlier.length)
                .ok_or(MapError::AddressOverflow)?;
            // Limine permits overlaps between non-usable regions, but
            // guarantees usable regions do not overlap any other entry.
            // Reject overlap that could make an allocatable span ambiguous.
            if region.base < earlier_end
                && earlier.base < end
                && (region.kind == USABLE_KIND || earlier.kind == USABLE_KIND)
            {
                return Err(MapError::OverlappingRegions);
            }
        }

        if region.kind != USABLE_KIND {
            continue;
        }
        let first = region.base.max(MIN_PHYSICAL_ADDRESS);
        let start = page_align_up(first).ok_or(MapError::AddressOverflow)?;
        let aligned_end = end & !(PAGE_BYTES - 1);
        if start >= aligned_end || (aligned_end - start) / PAGE_BYTES < min_pages {
            continue;
        }
        if choice.as_ref().is_none_or(|best| start < best.next) {
            choice = Some(FramePool {
                next: start,
                end: aligned_end,
            });
        }
    }
    choice.ok_or(MapError::NoUsableSpan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(base: u64, length: u64, kind: u64) -> Region {
        Region { base, length, kind }
    }

    #[test]
    fn concurrent_claims_have_one_winner_and_failure_consumes_permit() {
        use std::thread;

        let threads: Vec<_> = (0..8)
            .map(|_| thread::spawn(claim_boot_pool_permit))
            .collect();
        let mut permits: Vec<_> = threads
            .into_iter()
            .filter_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(permits.len(), 1);
        let permit = permits.pop().unwrap();
        assert_eq!(select_boot_pool(permit, &[], 1), Err(MapError::EmptyMap));
        assert_eq!(claim_boot_pool_permit(), None);
    }

    #[test]
    fn empty_map_and_zero_minimum_fail() {
        assert_eq!(choose_pool(&[], 1), Err(MapError::EmptyMap));
        assert_eq!(
            choose_pool(&[region(0x100000, PAGE_BYTES, 0)], 0),
            Err(MapError::InvalidMinimum)
        );
    }

    #[test]
    fn deterministic_selection_independent_of_entry_order() {
        let low = region(0x100000, 64 * PAGE_BYTES, 0);
        let high = region(0x400000, 32 * PAGE_BYTES, 0);
        let reserved = region(0x300000, PAGE_BYTES, 2);
        for map in [[high, reserved, low], [low, high, reserved]] {
            assert_eq!(choose_pool(&map, 16).unwrap().claim(), Some(0x100000));
        }
    }

    #[test]
    fn reserved_or_low_memory_is_never_claimed() {
        let map = [
            region(0, 0xa0000, 0),
            region(0x100000, 64 * PAGE_BYTES, 1),
            region(0x200000, 32 * PAGE_BYTES, 0),
        ];
        assert_eq!(choose_pool(&map, 16).unwrap().claim(), Some(0x200000));
    }

    #[test]
    fn alignment_rounds_inward() {
        let map = [region(0x100003, 17 * PAGE_BYTES + 10, 0)];
        let mut pool = choose_pool(&map, 16).unwrap();
        assert_eq!(pool.claim(), Some(0x101000));
        assert_eq!(pool.remaining(), 15);
    }

    #[test]
    fn zero_length_and_address_overflow_fail() {
        assert_eq!(
            choose_pool(&[region(0x100000, 0, 0)], 1),
            Err(MapError::ZeroLength)
        );
        assert_eq!(
            choose_pool(&[region(u64::MAX - 5, 10, 2)], 1),
            Err(MapError::AddressOverflow)
        );
    }

    #[test]
    fn usable_overlap_is_rejected_in_any_entry_order() {
        let usable = region(0x100000, 64 * PAGE_BYTES, USABLE_KIND);
        let reserved = region(0x102000, PAGE_BYTES, 1);
        let other_usable = region(0x103000, 2 * PAGE_BYTES, USABLE_KIND);
        for map in [
            [usable, reserved],
            [reserved, usable],
            [usable, other_usable],
        ] {
            assert_eq!(choose_pool(&map, 16), Err(MapError::OverlappingRegions));
        }
    }

    #[test]
    fn overlapping_nonusable_regions_do_not_poison_valid_usable_memory() {
        // A conforming Limine map may overlap non-usable region types.
        let reserved = region(0x100000, 8 * PAGE_BYTES, 1);
        let acpi = region(0x104000, 8 * PAGE_BYTES, 2);
        let usable = region(0x200000, 32 * PAGE_BYTES, USABLE_KIND);

        for map in [[reserved, acpi, usable], [usable, acpi, reserved]] {
            let mut pool = choose_pool(&map, 16).unwrap();
            assert_eq!(pool.claim(), Some(0x200000));
        }
    }

    #[test]
    fn adjacent_regions_are_not_overlapping() {
        let map = [
            region(0x100000, 16 * PAGE_BYTES, 0),
            region(0x110000, PAGE_BYTES, 1),
        ];
        assert!(choose_pool(&map, 16).is_ok());
    }

    #[test]
    fn too_short_or_entirely_reserved_fails() {
        assert_eq!(
            choose_pool(&[region(0x100000, 15 * PAGE_BYTES, 0)], 16),
            Err(MapError::NoUsableSpan)
        );
        assert_eq!(
            choose_pool(&[region(0x100000, 100 * PAGE_BYTES, 2)], 16),
            Err(MapError::NoUsableSpan)
        );
    }

    #[test]
    fn claimed_frames_are_unique_and_exhaustion_does_not_change_state() {
        let mut pool = choose_pool(&[region(0x100000, 16 * PAGE_BYTES, 0)], 16).unwrap();
        for index in 0..16 {
            assert_eq!(pool.claim(), Some(0x100000 + index * PAGE_BYTES));
        }
        assert_eq!(pool.remaining(), 0);
        assert_eq!(pool.claim(), None);
        assert_eq!(pool.claim(), None);
    }

    #[test]
    fn contiguous_reservation_preserves_addresses_and_disjoint_claims() {
        let mut pool = choose_pool(&[region(0x100000, 8 * PAGE_BYTES, 0)], 1).unwrap();
        assert_eq!(pool.claim(), Some(0x100000));

        let run = pool.reserve_contiguous(3).unwrap();
        assert_eq!(run.address_at(0), Some(0x101000));
        assert_eq!(run.address_at(1), Some(0x102000));
        assert_eq!(run.address_at(2), Some(0x103000));
        assert_eq!(run.address_at(3), None);
        assert_eq!(pool.claim(), Some(0x104000));
        assert_eq!(pool.remaining(), 3);
    }

    #[test]
    fn zero_length_reservation_does_not_consume_frames() {
        let mut pool = choose_pool(&[region(0x100000, 4 * PAGE_BYTES, 0)], 1).unwrap();
        assert_eq!(pool.reserve_contiguous(0), Err(ReservationError::ZeroPages));
        assert_eq!(pool.remaining(), 4);
        assert_eq!(pool.claim(), Some(0x100000));
    }

    #[test]
    fn insufficient_run_does_not_partially_reserve() {
        let mut pool = choose_pool(&[region(0x100000, 2 * PAGE_BYTES, 0)], 1).unwrap();
        assert_eq!(
            pool.reserve_contiguous(3),
            Err(ReservationError::InsufficientFrames)
        );
        assert_eq!(pool.remaining(), 2);
        let full_run = pool.reserve_contiguous(2).unwrap();
        assert_eq!(full_run.address_at(1), Some(0x101000));
        assert_eq!(pool.remaining(), 0);
        assert_eq!(
            pool.reserve_contiguous(1),
            Err(ReservationError::InsufficientFrames)
        );
        assert_eq!(pool.claim(), None);
    }

    #[test]
    fn oversized_reservation_arithmetic_cannot_wrap_or_advance() {
        let mut pool = choose_pool(&[region(0x100000, 4 * PAGE_BYTES, 0)], 1).unwrap();
        assert_eq!(
            pool.reserve_contiguous(u64::MAX),
            Err(ReservationError::AddressOverflow)
        );
        assert_eq!(pool.remaining(), 4);
        assert_eq!(pool.claim(), Some(0x100000));
    }

    #[test]
    fn dropping_reservation_does_not_reissue_the_run() {
        let mut pool = choose_pool(&[region(0x100000, 3 * PAGE_BYTES, 0)], 1).unwrap();
        {
            let run = pool.reserve_contiguous(2).unwrap();
            assert_eq!(run.address_at(0), Some(0x100000));
        }
        assert_eq!(pool.claim(), Some(0x102000));
        assert_eq!(pool.remaining(), 0);
    }

    #[test]
    fn near_u64_max_does_not_wrap() {
        assert_eq!(
            choose_pool(&[region(u64::MAX - 4095, 4095, 0)], 1),
            Err(MapError::NoUsableSpan)
        );
    }
}
