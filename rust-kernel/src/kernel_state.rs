//! Bootstrap-owned kernel state. This is not global physical-memory authority.
use crate::frame_registry::{FrameReservation, TransactionError, TransactionalFrameAllocator};
use crate::memory::{BootMapSnapshot, Region};

pub struct KernelState<const N: usize> {
    frames: TransactionalFrameAllocator<N>,
    boot_map: BootMapSnapshot,
}

impl<const N: usize> KernelState<N> {
    pub fn new(frames: TransactionalFrameAllocator<N>, boot_map: BootMapSnapshot) -> Self {
        Self { frames, boot_map }
    }

    /// Retained, copied Limine region metadata, not enforced resource authority.
    pub fn boot_regions(&self) -> &[Region] {
        self.boot_map.regions()
    }

    pub fn reservation(&self, index: usize) -> Option<FrameReservation> {
        self.frames.reservation(index)
    }

    #[allow(dead_code)]
    pub fn reserve(
        &mut self,
        pages: u64,
        owner: u32,
    ) -> Result<FrameReservation, TransactionError> {
        self.frames.reserve(pages, owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{choose_pool, Region, MIN_PHYSICAL_ADDRESS, PAGE_BYTES, USABLE_KIND};

    fn initialize() -> KernelState<2> {
        let usable = Region {
            base: MIN_PHYSICAL_ADDRESS,
            length: 4 * PAGE_BYTES,
            kind: USABLE_KIND,
        };
        let reserved = Region {
            base: 0x300000,
            length: PAGE_BYTES,
            kind: 1,
        };
        let pool = choose_pool(&[usable, reserved], 1).unwrap();
        let mut copied = [Region::EMPTY; crate::memory::MAX_BOOT_REGIONS];
        copied[0] = usable;
        copied[1] = reserved;
        let boot_map = BootMapSnapshot::from_copied_entries(copied, 2).unwrap();
        KernelState::new(
            TransactionalFrameAllocator::with_boot_reservation(pool).unwrap(),
            boot_map,
        )
    }

    // The unrestricted selector is test-only. Synthetic pools can overlap;
    // such a pool must never be treated as physical page-table authority.
    #[test]
    fn independently_constructed_pools_can_issue_the_same_frame() {
        let first = initialize().reservation(0).unwrap();
        let second = initialize().reservation(0).unwrap();
        assert_eq!(first.start, second.start);
    }

    #[test]
    fn copied_boot_memory_classifications_survive_state_move() {
        let state = initialize();
        assert_eq!(state.boot_regions().len(), 2);
        assert_eq!(state.boot_regions()[0].kind, USABLE_KIND);
        assert_eq!(state.boot_regions()[1].kind, 1);
        assert_eq!(state.boot_regions()[1].base, 0x300000);
    }

    #[test]
    fn moved_state_retains_boot_record_and_allocates_disjoint_next_run() {
        let mut state = initialize();
        let first = state.reservation(0).unwrap();
        let next = state.reserve(1, 2).unwrap();
        assert_eq!(next.start, first.start + 2 * PAGE_BYTES);
        assert_eq!(state.reservation(0), Some(first));
        assert_eq!(state.reservation(1), Some(next));
    }
}
