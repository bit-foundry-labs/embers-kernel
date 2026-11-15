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

const CR0_WP: u64 = 1 << 16;
const CPUID_NX: u32 = 1 << 20;
const EFER_NXE: u64 = 1 << 11;
const EFER_LMA: u64 = 1 << 10;
const MSR_EFER: u32 = 0xc000_0080;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectionError {
    MissingExtendedCpuid,
    InvalidPhysicalAddressWidth,
    InvalidLinearAddressWidth,
    NxUnsupported,
    WriteProtectDisabled,
    NxDisabled,
    LongModeInactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectionState {
    /// CPU-advertised MAXPHYADDR; not a proof of accessible RAM or frame ownership.
    pub physical_address_bits: u8,
    /// CPU-advertised maximum linear width; active mode is checked separately.
    pub linear_address_bits: u8,
}

/// Deterministic validation of sampled CPU permission-enforcement prerequisites.
///
/// This confirms reported bits, not the effectiveness of every leaf permission,
/// reserved-bit validity, or identity/lifetime of page-table backing.
pub fn validate_protection(
    max_extended_leaf: u32,
    feature_edx: u32,
    address_width_eax: u32,
    cr0: u64,
    efer: u64,
) -> Result<ProtectionState, ProtectionError> {
    if max_extended_leaf < 0x8000_0008 {
        return Err(ProtectionError::MissingExtendedCpuid);
    }
    let physical_address_bits = (address_width_eax & 0xff) as u8;
    let linear_address_bits = ((address_width_eax >> 8) & 0xff) as u8;
    if !(32..=52).contains(&physical_address_bits) {
        return Err(ProtectionError::InvalidPhysicalAddressWidth);
    }
    // A 4-level walk uses 48-bit canonical addresses even when the CPU
    // advertises support for a wider 5-level mode (disabled elsewhere).
    if linear_address_bits != 48 && linear_address_bits != 57 {
        return Err(ProtectionError::InvalidLinearAddressWidth);
    }
    if feature_edx & CPUID_NX == 0 {
        return Err(ProtectionError::NxUnsupported);
    }
    if cr0 & CR0_WP == 0 {
        return Err(ProtectionError::WriteProtectDisabled);
    }
    if efer & EFER_NXE == 0 {
        return Err(ProtectionError::NxDisabled);
    }
    if efer & EFER_LMA == 0 {
        return Err(ProtectionError::LongModeInactive);
    }
    Ok(ProtectionState {
        physical_address_bits,
        linear_address_bits,
    })
}

/// Only check the numeric alignment/width of a candidate root frame.
/// This does not authenticate the frame's allocation, mapping, or contents.
pub const fn root_candidate_fits_width(root: u64, bits: u8) -> bool {
    bits >= 32 && bits <= 52 && root != 0 && root & 0xfff == 0 && (root >> bits) == 0
}

/// Sample the BSP's CPU capability and permission state without modifying it.
///
/// # Safety boundary
/// Called only at CPL0 on the BSP after a valid Limine x86-64 long-mode
/// handoff. The EFER MSR is read only after CPUID has confirmed support for
/// the extended leaves; no new frame memory or device registers are touched.
pub fn sample_protection_state() -> Result<ProtectionState, ProtectionError> {
    use core::arch::x86_64::__cpuid;

    // SAFETY: CPUID is a processor query. We test the maximum reported
    // extended leaf before requesting feature and address-width leaves.
    let supported = unsafe { __cpuid(0x8000_0000) }.eax;
    if supported < 0x8000_0008 {
        return Err(ProtectionError::MissingExtendedCpuid);
    }
    // SAFETY: Both leaves were confirmed supported above.
    let features = unsafe { __cpuid(0x8000_0001) };
    // SAFETY: Leaf 0x80000008 is supported according to the CPU.
    let widths = unsafe { __cpuid(0x8000_0008) };
    if features.edx & CPUID_NX == 0 {
        return Err(ProtectionError::NxUnsupported);
    }

    let cr0: u64;
    let efer_low: u32;
    let efer_high: u32;
    // SAFETY: The single BSP is at CPL0, and the MSR exists in x86-64 long
    // mode. MOV-from-CR0 and RDMSR perform reads only.
    unsafe {
        asm!(
            "mov {value}, cr0",
            value = out(reg) cr0,
            options(nomem, nostack, preserves_flags)
        );
        asm!(
            "rdmsr",
            in("ecx") MSR_EFER,
            out("eax") efer_low,
            out("edx") efer_high,
            options(nomem, nostack, preserves_flags)
        );
    }
    let efer = ((efer_high as u64) << 32) | (efer_low as u64);
    validate_protection(supported, features.edx, widths.eax, cr0, efer)
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
    #[test]
    fn accepts_reported_physical_width_and_protection_controls() {
        let result = validate_protection(
            0x8000_0008,
            CPUID_NX,
            0x0000_3028, // 40-bit physical, 48-bit maximum linear.
            CR0_PG | CR0_WP,
            EFER_NXE | EFER_LMA,
        );
        assert_eq!(
            result,
            Ok(ProtectionState {
                physical_address_bits: 40,
                linear_address_bits: 48,
            })
        );
        assert!(root_candidate_fits_width(0x0ff9_c000, 40));
        assert!(!root_candidate_fits_width(1u64 << 40, 40));
        assert!(!root_candidate_fits_width(0x0ff9_c001, 40));
        assert!(!root_candidate_fits_width(0, 40));
    }

    #[test]
    fn rejects_incomplete_cpuid_and_invalid_widths() {
        let base = (CPUID_NX, 0x0000_3028, CR0_PG | CR0_WP, EFER_NXE | EFER_LMA);
        assert_eq!(
            validate_protection(0x8000_0007, base.0, base.1, base.2, base.3),
            Err(ProtectionError::MissingExtendedCpuid)
        );
        for width in [0u32, 31, 53] {
            assert_eq!(
                validate_protection(0x8000_0008, base.0, 0x3000 | width, base.2, base.3),
                Err(ProtectionError::InvalidPhysicalAddressWidth)
            );
        }
        for width in [0x2f, 49, 56, 58] {
            assert_eq!(
                validate_protection(
                    0x8000_0008,
                    base.0,
                    (width << 8) | 40,
                    base.2,
                    base.3,
                ),
                Err(ProtectionError::InvalidLinearAddressWidth)
            );
        }
        assert!(!root_candidate_fits_width(0x1000, 31));
        assert!(!root_candidate_fits_width(0x1000, 53));
    }

    #[test]
    fn rejects_disabled_or_unsupported_nx_and_wp() {
        let widths = 0x0000_3028;
        let present = EFER_NXE | EFER_LMA;
        assert_eq!(
            validate_protection(0x8000_0008, 0, widths, CR0_PG | CR0_WP, present),
            Err(ProtectionError::NxUnsupported)
        );
        assert_eq!(
            validate_protection(0x8000_0008, CPUID_NX, widths, CR0_PG, present),
            Err(ProtectionError::WriteProtectDisabled)
        );
        assert_eq!(
            validate_protection(0x8000_0008, CPUID_NX, widths, CR0_PG | CR0_WP, EFER_LMA),
            Err(ProtectionError::NxDisabled)
        );
        assert_eq!(
            validate_protection(0x8000_0008, CPUID_NX, widths, CR0_PG | CR0_WP, EFER_NXE),
            Err(ProtectionError::LongModeInactive)
        );
    }
}
