//! Bounded Kani harnesses for production FrameRegistry::reserve.
//! Standalone host-side verification: no privileged kernel or device access.
//! Run: kani rust-kernel/research/kani_frame_registry.rs
#![allow(dead_code)]

#[path = "../src/frame_registry.rs"]
mod frame_registry;
#[path = "../src/memory.rs"]
mod memory;

#[cfg(kani)]
mod verification {
    use super::frame_registry::{FrameRegistry, RegistryError};
    use super::memory::{MIN_PHYSICAL_ADDRESS, PAGE_BYTES};

    const BASE: u64 = MIN_PHYSICAL_ADDRESS;

    // Symbolic but bounded requests. Offsets include an intentionally
    // misaligned address. There is no assumption about result success.
    fn request() -> (u64, u64, u32) {
        let slot: u8 = kani::any();
        let pages: u8 = kani::any();
        let owner: u8 = kani::any();
        let misaligned: bool = kani::any();
        kani::assume(slot <= 5);
        kani::assume(pages <= 4);
        kani::assume(owner <= 2);

        let start = BASE + u64::from(slot) * PAGE_BYTES + u64::from(misaligned);
        (start, u64::from(pages), u32::from(owner))
    }

    #[kani::proof]
    fn rejected_request_preserves_every_record() {
        let mut registry = FrameRegistry::<2>::new();
        let first = registry.reserve(BASE, 2, 1).unwrap();
        let before_count = registry.count();
        let before_first = registry.entry(0);
        let before_second = registry.entry(1);
        let (start, pages, owner) = request();

        let result = registry.reserve(start, pages, owner);
        kani::cover!(result.is_ok());
        kani::cover!(result.is_err());

        if result.is_err() {
            assert_eq!(registry.count(), before_count);
            assert_eq!(registry.entry(0), before_first);
            assert_eq!(registry.entry(1), before_second);
            assert_eq!(registry.entry(0), Some(first));
        }
    }

    #[kani::proof]
    fn accepted_request_is_recorded_and_disjoint() {
        let mut registry = FrameRegistry::<2>::new();
        let first = registry.reserve(BASE, 2, 1).unwrap();
        let (start, pages, owner) = request();
        let result = registry.reserve(start, pages, owner);

        kani::cover!(result.is_ok());
        if let Ok(second) = result {
            assert_eq!(registry.count(), 2);
            assert_eq!(registry.entry(0), Some(first));
            assert_eq!(registry.entry(1), Some(second));
            assert_eq!(second.start, start);
            assert_eq!(second.pages, pages);
            assert_eq!(second.owner, owner);
            assert!(second.start >= first.start + first.pages * PAGE_BYTES);
        }
    }

    #[kani::proof]
    fn capacity_exhaustion_never_changes_state() {
        let mut registry = FrameRegistry::<1>::new();
        let first = registry.reserve(BASE, 1, 1).unwrap();
        let (start, pages, owner) = request();

        let result = registry.reserve(start, pages, owner);
        kani::cover!(matches!(result, Err(RegistryError::CapacityExhausted)));
        assert!(result.is_err());
        assert_eq!(registry.count(), 1);
        assert_eq!(registry.entry(0), Some(first));
    }
}

fn main() {}
