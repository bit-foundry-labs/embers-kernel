//! Pure host-only field checks for a present x86-64 4 KiB page-table path.
//!
//! These checks are intentionally **not** an architectural PTE decoder.
//! No CPU feature is detected and no physical memory is dereferenced.
//! The caller supplies MAXPHYADDR and the already-observed EFER.NXE state.
//! Execute permission, all reserved bits, PAT, PKU, PCID and device ownership
//! require additional explicit checks before privileged table installation.

/// Physical frame-address bits occupy 51:12 for ordinary 4 KiB entries.
const PHYSICAL_FIELD: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const BIT_7: u64 = 1 << 7;
const NX: u64 = 1 << 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableLevel {
    Pml4,
    Pdpt,
    Pd,
    Pt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldError {
    InvalidPhysicalWidth,
    RootFrameZero,
    NotPresent,
    ReservedPhysicalAddressBits,
    UnsupportedLargePageOrReservedBit7,
    NxWithoutNxe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryBits {
    /// Numeric frame field, not an owned, allocated or mapped frame.
    pub frame_candidate: u64,
    pub writable: bool,
    pub user: bool,
    pub execute_disable: bool,
}

/// Extract only the physical-address field allowed by a caller-supplied width.
///
/// Bits 62:52 may have unrelated architectural or software-defined uses, and
/// are not incorrectly classified here as extra physical-address bits.
/// This helper therefore does not certify a complete entry's validity.
fn permitted_field(physical_bits: u8) -> Result<u64, FieldError> {
    if !(32..=52).contains(&physical_bits) {
        return Err(FieldError::InvalidPhysicalWidth);
    }
    Ok(((1u64 << physical_bits) - 1) & PHYSICAL_FIELD)
}

/// Inspect a raw CR3 root frame against MAXPHYADDR without parsing CR3 flags.
///
/// Low CR3 bits depend on CR4.PCIDE and other architectural state. High CR3
/// control/reserved fields are likewise outside this bounded check. This is
/// **not** a complete CR3 validator or permission to switch page-table roots.
pub fn cr3_root_candidate(raw_cr3: u64, physical_bits: u8) -> Result<u64, FieldError> {
    let allowed = permitted_field(physical_bits)?;
    if raw_cr3 & PHYSICAL_FIELD & !allowed != 0 {
        return Err(FieldError::ReservedPhysicalAddressBits);
    }
    let frame = raw_cr3 & allowed;
    if frame == 0 {
        // Embers research policy, not a general architectural ban on frame 0.
        return Err(FieldError::RootFrameZero);
    }
    Ok(frame)
}

/// Validate a restricted, *present 4 KiB page-path* entry's selected fields.
///
/// Reject any physical-address bits 51:MAXPHYADDR (exclusive upper bound).
/// Intermediate PS/bit-7 is deliberately unsupported. PT bit 7 is the PAT
/// field, whose behavior is NOT validated here. Bits 62:52, page cache
/// policy, protection keys, TLB semantics, U/S and effective access rights
/// are not completely interpreted. Presence and reported bits are not proof
/// of lifetime, frame ownership, read access or protection enforcement.
pub fn present_4k_entry(
    raw: u64,
    level: TableLevel,
    physical_bits: u8,
    nxe_enabled: bool,
) -> Result<EntryBits, FieldError> {
    let allowed = permitted_field(physical_bits)?;
    if raw & PRESENT == 0 {
        return Err(FieldError::NotPresent);
    }
    if raw & PHYSICAL_FIELD & !allowed != 0 {
        return Err(FieldError::ReservedPhysicalAddressBits);
    }
    if !matches!(level, TableLevel::Pt) && raw & BIT_7 != 0 {
        return Err(FieldError::UnsupportedLargePageOrReservedBit7);
    }
    if !nxe_enabled && raw & NX != 0 {
        return Err(FieldError::NxWithoutNxe);
    }
    Ok(EntryBits {
        frame_candidate: raw & allowed,
        writable: raw & WRITABLE != 0,
        user: raw & USER != 0,
        execute_disable: raw & NX != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: u64 = 0x0000_0000_0ff4_4001;

    #[test]
    fn reports_frame_and_raw_permission_bits_on_every_level() {
        for level in [
            TableLevel::Pml4,
            TableLevel::Pdpt,
            TableLevel::Pd,
            TableLevel::Pt,
        ] {
            assert_eq!(
                present_4k_entry(P | WRITABLE | NX, level, 40, true),
                Ok(EntryBits {
                    frame_candidate: 0x0ff4_4000,
                    writable: true,
                    user: false,
                    execute_disable: true,
                })
            );
        }
    }

    #[test]
    fn rejects_reserved_physical_address_bits_on_all_levels() {
        let reserved_bit = 1u64 << 40;
        for level in [
            TableLevel::Pml4,
            TableLevel::Pdpt,
            TableLevel::Pd,
            TableLevel::Pt,
        ] {
            assert_eq!(
                present_4k_entry(P | reserved_bit, level, 40, true),
                Err(FieldError::ReservedPhysicalAddressBits)
            );
        }
        assert_eq!(
            cr3_root_candidate(P | reserved_bit, 40),
            Err(FieldError::ReservedPhysicalAddressBits)
        );
        assert_eq!(cr3_root_candidate(P, 40), Ok(0x0ff4_4000));
    }

    #[test]
    fn width_boundaries_reject_invalid_and_accept_maximum_frame_bit() {
        for width in [0, 12, 31, 53, 255] {
            assert_eq!(
                present_4k_entry(P, TableLevel::Pt, width, true),
                Err(FieldError::InvalidPhysicalWidth)
            );
            assert_eq!(
                cr3_root_candidate(P, width),
                Err(FieldError::InvalidPhysicalWidth)
            );
        }
        assert_eq!(
            present_4k_entry((1u64 << 51) | PRESENT, TableLevel::Pt, 52, true)
                .unwrap()
                .frame_candidate,
            1u64 << 51
        );
        assert_eq!(
            present_4k_entry(P | (1u64 << 39), TableLevel::Pt, 40, true)
                .unwrap()
                .frame_candidate,
            0x0ff4_4000 | (1u64 << 39)
        );
    }

    #[test]
    fn rejects_absent_large_and_unsupported_nx_entries() {
        assert_eq!(
            present_4k_entry(P & !PRESENT, TableLevel::Pt, 40, true),
            Err(FieldError::NotPresent)
        );
        for level in [TableLevel::Pml4, TableLevel::Pdpt, TableLevel::Pd] {
            assert_eq!(
                present_4k_entry(P | BIT_7, level, 40, true),
                Err(FieldError::UnsupportedLargePageOrReservedBit7)
            );
        }
        assert_eq!(
            present_4k_entry(P | NX, TableLevel::Pt, 40, false),
            Err(FieldError::NxWithoutNxe)
        );
        // PT bit 7 is a PAT field, *not* a large-page entry. PAT semantics
        // remain outside this model rather than being falsely classified.
        assert!(present_4k_entry(P | BIT_7, TableLevel::Pt, 40, true).is_ok());
    }

    #[test]
    fn preserves_nonaddress_software_bits_without_mislabeling_them_reserved() {
        let raw = P | (1u64 << 55) | USER;
        let result = present_4k_entry(raw, TableLevel::Pt, 40, true).unwrap();
        assert_eq!(result.frame_candidate, 0x0ff4_4000);
        assert!(result.user);
        assert_eq!(
            cr3_root_candidate(0x0000_0000_0ff4_4123, 40),
            Ok(0x0ff4_4000)
        );
        assert_eq!(
            cr3_root_candidate(0x123, 40),
            Err(FieldError::RootFrameZero)
        );
    }
}
