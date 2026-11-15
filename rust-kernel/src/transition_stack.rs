//! Experimental stack storage, not an active RSP or hardware isolation proof.
//! The feature-only QEMU probe touches kernel BSS under Limine's mappings.

pub const PAGE_BYTES: usize = 4096;
pub const STACK_PAGES: usize = 4;
pub const STACK_BYTES: usize = PAGE_BYTES * STACK_PAGES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackProbeError {
    NullAddress,
    WrongLength,
    Unaligned,
    AddressOverflow,
    NonCanonical,
    ReadbackMismatch,
}

const fn is_canonical_48(address: u64) -> bool {
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if sign == 0 {
        upper == 0
    } else {
        upper == 0xffff
    }
}

/// Numeric range validation only, NOT mapping or ownership evidence.
pub fn validate_storage_range(base: u64, bytes: usize) -> Result<(), StackProbeError> {
    if base == 0 {
        return Err(StackProbeError::NullAddress);
    }
    if bytes != STACK_BYTES {
        return Err(StackProbeError::WrongLength);
    }
    if base & ((PAGE_BYTES as u64) - 1) != 0 {
        return Err(StackProbeError::Unaligned);
    }
    let last = base
        .checked_add((bytes - 1) as u64)
        .ok_or(StackProbeError::AddressOverflow)?;
    if !is_canonical_48(base) || !is_canonical_48(last) {
        return Err(StackProbeError::NonCanonical);
    }
    Ok(())
}

#[cfg(feature = "transition-stack-smoke")]
#[repr(align(4096))]
struct AlignedStack([u8; STACK_BYTES]);

// Kernel-image BSS, not an allocation from the boot frame registry.
// This experiment never switches the CPU stack into this storage.
#[cfg(feature = "transition-stack-smoke")]
static mut TRANSITION_STACK: AlignedStack = AlignedStack([0; STACK_BYTES]);

/// Probe the first and last byte of each 4 KiB page, restoring original bytes.
///
/// # Safety
/// Call once on the single BSP with interrupts disabled and Limine's writable
/// BSS mappings active. No other code may access this private static. An
/// invalid inherited mapping can produce a fatal page fault in this opt-in
/// QEMU test. Never call it from the normal boot configuration.
#[cfg(feature = "transition-stack-smoke")]
pub unsafe fn probe_inherited_mappings() -> Result<u64, StackProbeError> {
    use core::ptr::{addr_of_mut, read_volatile, write_volatile};
    // SAFETY: This feature-only probe runs exclusively on the BSP, and only
    // forms a raw pointer to private BSS storage. It creates no Rust reference.
    let storage = unsafe { addr_of_mut!(TRANSITION_STACK.0).cast::<u8>() };
    validate_storage_range(storage as u64, STACK_BYTES)?;

    for page in 0..STACK_PAGES {
        for within_page in [0, PAGE_BYTES - 1] {
            let offset = page * PAGE_BYTES + within_page;
            // SAFETY: The addresses remain within the private static; this
            // BSP-only QEMU probe assumes Limine's writable BSS mappings.
            unsafe {
                let location = storage.add(offset);
                let original = read_volatile(location);
                let changed = original ^ (0xa5 ^ page as u8);
                write_volatile(location, changed);
                let observed = read_volatile(location);
                write_volatile(location, original);
                if observed != changed {
                    return Err(StackProbeError::ReadbackMismatch);
                }
            }
        }
    }
    Ok(storage as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_aligned_canonical_ranges_in_either_half() {
        for start in [0x0040_0000, 0xffff_ffff_8000_0000] {
            assert_eq!(validate_storage_range(start, STACK_BYTES), Ok(()));
        }
    }

    #[test]
    fn rejects_null_unaligned_and_wrong_length() {
        assert_eq!(
            validate_storage_range(0, STACK_BYTES),
            Err(StackProbeError::NullAddress)
        );
        assert_eq!(
            validate_storage_range(0x1001, STACK_BYTES),
            Err(StackProbeError::Unaligned)
        );
        assert_eq!(
            validate_storage_range(0x1000, STACK_BYTES - 1),
            Err(StackProbeError::WrongLength)
        );
    }

    #[test]
    fn rejects_noncanonical_start_or_end() {
        assert_eq!(
            validate_storage_range(0x0000_8000_0000_0000, STACK_BYTES),
            Err(StackProbeError::NonCanonical)
        );
        assert_eq!(
            validate_storage_range(0x0000_7fff_ffff_f000, STACK_BYTES),
            Err(StackProbeError::NonCanonical)
        );
    }

    #[test]
    fn rejects_address_overflow() {
        assert_eq!(
            validate_storage_range(0xffff_ffff_ffff_f000, STACK_BYTES),
            Err(StackProbeError::AddressOverflow)
        );
    }
}
