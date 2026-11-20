//! Bounded Kani checks of the production transactional frame allocator.
//! No physical-memory access, kernel hardware change, or replacement model.
//! Run from repository root: kani rust-kernel/research/kani_transactional_allocator.rs
#![allow(dead_code)]
// Standalone Kani may compile without the usual Cargo extern prelude.
extern crate core;

#[path = "../src/frame_registry.rs"]
mod frame_registry;
#[path = "../src/memory.rs"]
mod memory;

#[cfg(kani)]
mod verification {
    use super::frame_registry::{
        FrameReservation, RegistryError, TransactionError, TransactionalFrameAllocator,
    };
    use super::memory::{
        claim_boot_pool_permit, select_boot_pool, Region, ReservationError, MIN_PHYSICAL_ADDRESS,
        PAGE_BYTES, USABLE_KIND,
    };

    const BASE: u64 = MIN_PHYSICAL_ADDRESS;
    const POOL_PAGES: u64 = 8;

    // Use the real one-shot boot permit and production pool constructor.
    // In this harness the region is *assumed* eligible and wholly available;
    // no actual Limine response, mapping, or physical access is verified.
    fn allocator<const N: usize>() -> TransactionalFrameAllocator<N> {
        let permit = claim_boot_pool_permit().unwrap();
        let pool = select_boot_pool(
            permit,
            &[Region {
                base: BASE,
                length: POOL_PAGES * PAGE_BYTES,
                kind: USABLE_KIND,
            }],
            1,
        )
        .unwrap();
        TransactionalFrameAllocator::<N>::with_boot_reservation(pool).unwrap()
    }

    fn initial_reservation() -> FrameReservation {
        FrameReservation {
            start: BASE,
            pages: 2,
            owner: 1,
        }
    }

    #[kani::proof]
    fn rejection_preserves_pool_and_every_exposed_slot() {
        let mut frames = allocator::<2>();
        let before_pages = frames.remaining_pages();
        let before_first = frames.reservation(0);
        let before_second = frames.reservation(1);
        let pages: u8 = kani::any();
        let owner: u8 = kani::any();
        kani::assume(pages <= 10);
        kani::assume(owner <= 2);

        let result = frames.reserve(u64::from(pages), u32::from(owner));

        kani::cover!(result.is_ok());
        kani::cover!(matches!(
            result,
            Err(TransactionError::Pool(ReservationError::ZeroPages))
        ));
        kani::cover!(matches!(
            result,
            Err(TransactionError::Pool(ReservationError::InsufficientFrames))
        ));
        kani::cover!(matches!(
            result,
            Err(TransactionError::Registry(RegistryError::InvalidOwner))
        ));
        if result.is_err() {
            assert_eq!(frames.remaining_pages(), before_pages);
            assert_eq!(frames.reservation(0), before_first);
            assert_eq!(frames.reservation(1), before_second);
            assert_eq!(frames.reservation(0), Some(initial_reservation()));
        }
    }

    #[kani::proof]
    fn accepted_reservation_consumes_exact_number_of_frames() {
        let mut frames = allocator::<2>();
        let initial = frames.reservation(0).unwrap();
        let remaining_before = frames.remaining_pages();
        let pages: u8 = kani::any();
        let owner: u8 = kani::any();
        kani::assume((1..=6).contains(&pages));
        kani::assume((1..=2).contains(&owner));

        let record = frames.reserve(u64::from(pages), u32::from(owner)).unwrap();

        assert_eq!(initial, initial_reservation());
        assert_eq!(remaining_before, POOL_PAGES - initial.pages);
        assert_eq!(frames.remaining_pages(), remaining_before - u64::from(pages));
        assert_eq!(record.start, BASE + initial.pages * PAGE_BYTES);
        assert_eq!(record.pages, u64::from(pages));
        assert_eq!(record.owner, u32::from(owner));
        assert_eq!(frames.reservation(0), Some(initial));
        assert_eq!(frames.reservation(1), Some(record));
        assert!(record.start + record.pages * PAGE_BYTES <= BASE + POOL_PAGES * PAGE_BYTES);
    }

    #[kani::proof]
    fn full_registry_rejects_after_pool_preflight_without_consumption() {
        // The initial two-page boot reservation fills the only registry slot.
        // A requested run of 1..6 pages passes the pool preview.
        let mut frames = allocator::<1>();
        let initial = frames.reservation(0);
        let remaining_before = frames.remaining_pages();
        let pages: u8 = kani::any();
        kani::assume((1..=6).contains(&pages));

        let result = frames.reserve(u64::from(pages), 2);
        assert_eq!(
            result,
            Err(TransactionError::Registry(RegistryError::CapacityExhausted))
        );
        assert_eq!(frames.remaining_pages(), remaining_before);
        assert_eq!(frames.reservation(0), initial);
        assert_eq!(frames.reservation(1), None);
    }

    #[kani::proof]
    fn full_width_page_count_cannot_consume_frames_on_rejection() {
        // Symbolic 64-bit count includes multiplication-overflow cases.
        // This isolates the arithmetic boundary without assuming a page limit.
        let mut frames = allocator::<2>();
        let before_pages = frames.remaining_pages();
        let before_first = frames.reservation(0);
        let before_second = frames.reservation(1);
        let pages: u64 = kani::any();

        let result = frames.reserve(pages, 1);
        kani::cover!(matches!(
            result,
            Err(TransactionError::Pool(ReservationError::AddressOverflow))
        ));
        kani::cover!(result.is_ok());
        if result.is_err() {
            assert_eq!(frames.remaining_pages(), before_pages);
            assert_eq!(frames.reservation(0), before_first);
            assert_eq!(frames.reservation(1), before_second);
        }
    }
}

fn main() {}
