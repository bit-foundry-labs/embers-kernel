//! Read-only x86-64 paging preflight for M06b; no table writes or CR3 changes.
//! This validates selected control-register bits, not page-table contents.
use core::arch::asm;

const CR0_PG: u64 = 1 << 31;
const CR4_PAE: u64 = 1 << 5;
const CR4_LA57: u64 = 1 << 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PagingModeError {
    PagingDisabled,
    PhysicalAddressExtensionDisabled,
    FiveLevelPagingUnsupported,
}

/// Pure bit-level validation of the current four-level paging prerequisites.
///
/// The Limine x86-64 long-mode handoff is a separate trust assumption.
/// This check does not inspect EFER, CR3, translations, or page permissions.
pub fn validate(cr0: u64, cr4: u64) -> Result<(), PagingModeError> {
    if cr0 & CR0_PG == 0 {
        return Err(PagingModeError::PagingDisabled);
    }
    if cr4 & CR4_PAE == 0 {
        return Err(PagingModeError::PhysicalAddressExtensionDisabled);
    }
    if cr4 & CR4_LA57 != 0 {
        return Err(PagingModeError::FiveLevelPagingUnsupported);
    }
    Ok(())
}

/// Sample the BSP's current paging bits without modifying CPU state.
///
/// # Safety boundary
/// This function is only valid at CPL0 after Limine's x86-64 handoff, on
/// the bootstrap processor. CR reads would fault in an unprivileged caller.
pub fn check_active() -> Result<(), PagingModeError> {
    let cr0: u64;
    let cr4: u64;
    // SAFETY: Called only from the single privileged early-boot BSP path.
    // These instructions read control registers and do not change mappings.
    unsafe {
        asm!(
            "mov {value}, cr0",
            value = out(reg) cr0,
            options(nomem, nostack, preserves_flags)
        );
        asm!(
            "mov {value}, cr4",
            value = out(reg) cr4,
            options(nomem, nostack, preserves_flags)
        );
    }
    validate(cr0, cr4)
}

/// Read-only numeric observations made on the BSP under Limine's active tables.
///
/// Neither value authenticates a frame, establishes a mapped stack extent,
/// nor proves that new page tables could preserve live mappings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootPagingSnapshot {
    pub raw_cr3: u64,
    pub observed_rsp: u64,
}

impl BootPagingSnapshot {
    /// CR3 bits 51:12 identify the root 4 KiB frame on this x86-64 model.
    /// Low bits can encode control flags or a PCID. Physical-width validation,
    /// table content and ownership are NOT established by masking.
    pub const fn root_frame_candidate(self) -> u64 {
        self.raw_cr3 & 0x000f_ffff_ffff_f000
    }

    /// Check only the current four-level, 48-bit canonical-address encoding.
    /// This does not establish that the stack is mapped or kernel-owned.
    pub const fn rsp_is_canonical_48(self) -> bool {
        let upper = self.observed_rsp >> 48;
        let sign = (self.observed_rsp >> 47) & 1;
        if sign == 0 {
            upper == 0
        } else {
            upper == 0xffff
        }
    }
}

/// Observe CR3 and the RSP at the sampling instruction, without changing them.
///
/// # Safety boundary
/// Called only after Limine has entered a single CPL0 BSP and the paging-mode
/// check has accepted four-level paging. The RSP sample is NOT the original
/// Limine entry stack pointer, stack base, or proof of stack lifetime.
pub fn sample_active_context() -> BootPagingSnapshot {
    let raw_cr3: u64;
    let observed_rsp: u64;
    // SAFETY: Only called on the privileged BSP; both instructions are reads.
    unsafe {
        asm!(
            "mov {value}, cr3",
            value = out(reg) raw_cr3,
            options(nomem, nostack, preserves_flags)
        );
        asm!(
            "mov {value}, rsp",
            value = out(reg) observed_rsp,
            options(nomem, nostack, preserves_flags)
        );
    }
    BootPagingSnapshot {
        raw_cr3,
        observed_rsp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_four_level_paging_and_unrelated_control_bits() {
        assert_eq!(validate(CR0_PG, CR4_PAE), Ok(()));
        assert_eq!(validate(CR0_PG | 1, CR4_PAE | (1 << 7)), Ok(()));
    }

    #[test]
    fn rejects_disabled_paging() {
        assert_eq!(validate(0, CR4_PAE), Err(PagingModeError::PagingDisabled));
    }

    #[test]
    fn rejects_missing_pae() {
        assert_eq!(
            validate(CR0_PG, 0),
            Err(PagingModeError::PhysicalAddressExtensionDisabled)
        );
    }

    #[test]
    fn rejects_five_level_paging() {
        assert_eq!(
            validate(CR0_PG, CR4_PAE | CR4_LA57),
            Err(PagingModeError::FiveLevelPagingUnsupported)
        );
    }

    #[test]
    fn rejection_priority_is_deterministic() {
        assert_eq!(validate(0, CR4_LA57), Err(PagingModeError::PagingDisabled));
        assert_eq!(
            validate(CR0_PG, CR4_LA57),
            Err(PagingModeError::PhysicalAddressExtensionDisabled)
        );
    }
    #[test]
    fn root_candidate_masks_cr3_control_bits_without_claiming_ownership() {
        let snapshot = BootPagingSnapshot {
            raw_cr3: 0x0000_0001_2345_6123,
            observed_rsp: 0xffff_8000_0010_0000,
        };
        assert_eq!(snapshot.root_frame_candidate(), 0x0000_0001_2345_6000);
        assert!(snapshot.rsp_is_canonical_48());
    }

    #[test]
    fn stack_canonicality_rejects_both_48_bit_holes() {
        for rsp in [0, 0x0000_7fff_ffff_ffff, 0xffff_8000_0000_0000] {
            assert!(BootPagingSnapshot {
                raw_cr3: 0,
                observed_rsp: rsp
            }
            .rsp_is_canonical_48());
        }
        for rsp in [0x0000_8000_0000_0000, 0xffff_0000_0000_0000] {
            assert!(!BootPagingSnapshot {
                raw_cr3: 0,
                observed_rsp: rsp
            }
            .rsp_is_canonical_48());
        }
    }
}
