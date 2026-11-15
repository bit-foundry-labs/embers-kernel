//! Feature-only transition-stack research under inherited Limine mappings.
//! An optional one-shot RSP switch remains QEMU-only, without any CR3 change.

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
    CallbackOutsideStack,
    OriginalStackNotRestored,
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

#[cfg(any(feature = "transition-stack-smoke", feature = "transition-stack-switch"))]
#[repr(align(4096))]
struct AlignedStack([u8; STACK_BYTES]);

// Kernel-image BSS, not an allocation from the boot frame registry.
// The switch feature uses this same static only in disposable QEMU.
#[cfg(any(feature = "transition-stack-smoke", feature = "transition-stack-switch"))]
static mut TRANSITION_STACK: AlignedStack = AlignedStack([0; STACK_BYTES]);

/// Probe the first and last byte of each 4 KiB page, restoring original bytes.
///
/// # Safety
/// Call once on the single BSP with interrupts disabled and Limine's writable
/// BSS mappings active. No other code may access this private static. An
/// invalid inherited mapping can produce a fatal page fault in this opt-in
/// QEMU test. Never call it from the normal boot configuration.
#[cfg(any(feature = "transition-stack-smoke", feature = "transition-stack-switch"))]
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


#[cfg(feature = "transition-stack-switch")]
core::arch::global_asm!(
    ".global embers_switch_stack_once",
    ".type embers_switch_stack_once, @function",
    "embers_switch_stack_once:",
    "push rbx",
    "mov rbx, rsp",
    "mov rsp, rdi",
    "and rsp, -16",
    "call rsi",
    "mov rsp, rbx",
    "pop rbx",
    "ret",
    ".size embers_switch_stack_once, .-embers_switch_stack_once",
);

#[cfg(feature = "transition-stack-switch")]
extern "C" {
    fn embers_switch_stack_once(top: u64, callback: extern "C" fn() -> u64) -> u64;
}

#[cfg(feature = "transition-stack-switch")]
#[inline(never)]
extern "C" fn witness_callback_rsp() -> u64 {
    let observed: u64;
    // SAFETY: RSP is read inside the ABI callback after CALL placed its
    // return address on the temporary BSS stack. No memory is dereferenced.
    unsafe {
        core::arch::asm!(
            "mov {observed}, rsp",
            observed = out(reg) observed,
            options(nomem, nostack, preserves_flags)
        );
    }
    observed
}

/// Pure numeric outcome checking; does not certify backing or ownership.
pub fn validate_stack_switch_sample(
    base: u64,
    bytes: usize,
    callback_rsp: u64,
    original_rsp: u64,
    restored_rsp: u64,
) -> Result<(), StackProbeError> {
    validate_storage_range(base, bytes)?;
    if original_rsp != restored_rsp {
        return Err(StackProbeError::OriginalStackNotRestored);
    }
    // Expect a shallow, naturally aligned callback near the top page.
    // A deeper/unexpected stack frame fails closed without changing CR3.
    let top = base + bytes as u64;
    if callback_rsp < top - PAGE_BYTES as u64
        || callback_rsp >= top
        || callback_rsp % 8 != 0
        || !is_canonical_48(callback_rsp)
    {
        return Err(StackProbeError::CallbackOutsideStack);
    }
    Ok(())
}

/// Switch RSP once to private static BSS, invoke a no-argument ABI callback,
/// restore the saved bootstrap RSP, and validate the observed result.
///
/// # Safety
/// Only a disposable QEMU feature build may call this after boot and paging
/// preflight on the single CPL0 BSP with interrupts disabled. The entire static
/// stack must be writable under inherited Limine mappings. The callback must
/// obey the SysV callee-saved register ABI and return normally; an exception
/// is fatal, and this is not an atomic rollback or a root-switch protocol.
///
/// The assembly preserves RBX on the old stack and uses it to restore RSP
/// before RET. No ordinary Rust frame is active while RSP points elsewhere.
#[cfg(feature = "transition-stack-switch")]
pub unsafe fn probe_stack_switch() -> Result<(u64, u64), StackProbeError> {
    let base = unsafe { probe_inherited_mappings()? };
    let top = base + STACK_BYTES as u64;
    let before: u64;
    let after: u64;
    // SAFETY: The single BSP owns both mapped stacks under inherited CR3.
    // The assembly swaps RSP around a SysV callback without executing Rust
    // instructions on the unrecognized stack, then restores the old RSP.
    let callback_rsp = unsafe {
        core::arch::asm!(
            "mov {rsp}, rsp",
            rsp = out(reg) before,
            options(nomem, nostack, preserves_flags)
        );
        let callback_rsp = embers_switch_stack_once(top, witness_callback_rsp);
        core::arch::asm!(
            "mov {rsp}, rsp",
            rsp = out(reg) after,
            options(nomem, nostack, preserves_flags)
        );
        callback_rsp
    };
    validate_stack_switch_sample(base, STACK_BYTES, callback_rsp, before, after)?;
    Ok((base, callback_rsp))
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
    fn accepts_callback_inside_last_page_and_preserved_bootstrap_rsp() {
        let base = 0xffff_ffff_8000_0000;
        let old = 0xffff_8000_0001_0000;
        assert_eq!(
            validate_stack_switch_sample(
                base,
                STACK_BYTES,
                base + STACK_BYTES as u64 - 8,
                old,
                old
            ),
            Ok(())
        );
    }

    #[test]
    fn rejects_callback_outside_dedicated_stack_or_unaligned() {
        let base = 0xffff_ffff_8000_0000;
        let old = 0xffff_8000_0001_0000;
        for rsp in [base, base + STACK_BYTES as u64, base + 7,
                    base + STACK_BYTES as u64 - 7, 0x0000_8000_0000_0000] {
            assert_eq!(
                validate_stack_switch_sample(base, STACK_BYTES, rsp, old, old),
                Err(StackProbeError::CallbackOutsideStack)
            );
        }
    }

    #[test]
    fn rejects_unrestored_bootstrap_rsp_before_accepting_callback() {
        let base = 0xffff_ffff_8000_0000;
        let old = 0xffff_8000_0001_0000;
        assert_eq!(
            validate_stack_switch_sample(
                base,
                STACK_BYTES,
                base + STACK_BYTES as u64 - 8,
                old,
                old - 8
            ),
            Err(StackProbeError::OriginalStackNotRestored)
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
