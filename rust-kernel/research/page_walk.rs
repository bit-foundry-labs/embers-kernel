//! Host-only, single-path four-level translation model. No hardware mappings.
//! Supervisor writes follow a restrictive CR0.WP-enabled assumption; this
//! model does not read control registers or cover all x86 permission rules.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub target: u64,
    pub present: bool,
    pub writable: bool,
    pub user: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WalkError {
    NonCanonical,
    InvalidTableAddress,
    MissingEntry,
    InvalidLeafAddress,
    WriteDenied,
    UserDenied,
}

/// Each array represents one table at its stated synthetic physical address.
/// This deliberately does not read physical memory or model reserved bits.
pub struct Hierarchy {
    pub table_addresses: [u64; 4],
    pub tables: [[Entry; 512]; 4],
}

pub fn indices(virtual_address: u64) -> Result<[usize; 4], WalkError> {
    let top = virtual_address >> 48;
    let sign = (virtual_address >> 47) & 1;
    if (sign == 0 && top != 0) || (sign == 1 && top != 0xffff) {
        return Err(WalkError::NonCanonical);
    }
    Ok([39, 30, 21, 12].map(|shift| ((virtual_address >> shift) & 0x1ff) as usize))
}

impl Hierarchy {
    pub fn translate(
        &self,
        virtual_address: u64,
        write: bool,
        user: bool,
    ) -> Result<u64, WalkError> {
        let index = indices(virtual_address)?;
        // This single-path model excludes recursive or shared table frames.
        for (level, address) in self.table_addresses.iter().enumerate() {
            if address & 0xfff != 0 || self.table_addresses[..level].contains(address) {
                return Err(WalkError::InvalidTableAddress);
            }
        }
        let mut can_write = true;
        let mut can_user = true;
        for (level, slot) in index.iter().copied().enumerate() {
            let entry = self.tables[level][slot];
            if !entry.present {
                return Err(WalkError::MissingEntry);
            }
            can_write &= entry.writable;
            can_user &= entry.user;
            if level < 3 {
                if entry.target & 0xfff != 0 || entry.target != self.table_addresses[level + 1] {
                    return Err(WalkError::InvalidTableAddress);
                }
            } else {
                if entry.target & 0xfff != 0 {
                    return Err(WalkError::InvalidLeafAddress);
                }
                if write && !can_write {
                    return Err(WalkError::WriteDenied);
                }
                if user && !can_user {
                    return Err(WalkError::UserDenied);
                }
                return Ok(entry.target | (virtual_address & 0xfff));
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: Entry = Entry {
        target: 0,
        present: false,
        writable: false,
        user: false,
    };

    fn hierarchy(va: u64) -> Hierarchy {
        let mut h = Hierarchy {
            table_addresses: [0x1000, 0x2000, 0x3000, 0x4000],
            tables: [[EMPTY; 512]; 4],
        };
        let ix = indices(va).unwrap();
        for (level, slot) in ix.iter().copied().enumerate() {
            h.tables[level][slot] = Entry {
                target: if level == 3 {
                    0x9000
                } else {
                    h.table_addresses[level + 1]
                },
                present: true,
                writable: true,
                user: true,
            };
        }
        h
    }

    #[test]
    fn translates_both_canonical_halves_and_preserves_offset() {
        for va in [0x1234_5678_9abcu64, 0xffff_8000_1234_5abcu64] {
            assert_eq!(hierarchy(va).translate(va, true, true), Ok(0x9abc));
        }
    }

    #[test]
    fn rejects_noncanonical_missing_conflicting_and_unaligned_entries() {
        let va = 0x1234_5678_9abcu64;
        assert_eq!(indices(0x0000_8000_0000_0000), Err(WalkError::NonCanonical));
        assert_eq!(indices(0xffff_0000_0000_0000), Err(WalkError::NonCanonical));
        let ix = indices(va).unwrap();
        let mut h = hierarchy(va);
        h.tables[1][ix[1]].present = false;
        assert_eq!(h.translate(va, false, false), Err(WalkError::MissingEntry));
        h = hierarchy(va);
        h.tables[1][ix[1]].target = 0x5000;
        assert_eq!(
            h.translate(va, false, false),
            Err(WalkError::InvalidTableAddress)
        );
        h = hierarchy(va);
        h.tables[3][ix[3]].target = 0x9001;
        assert_eq!(
            h.translate(va, false, false),
            Err(WalkError::InvalidLeafAddress)
        );
    }

    #[test]
    fn rejects_aliased_or_unaligned_table_frames() {
        let va = 0x1234_5678_9abcu64;
        let mut h = hierarchy(va);
        h.table_addresses[0] = h.table_addresses[2];
        assert_eq!(
            h.translate(va, false, false),
            Err(WalkError::InvalidTableAddress)
        );
        h = hierarchy(va);
        h.table_addresses[0] = 0x1001;
        assert_eq!(
            h.translate(va, false, false),
            Err(WalkError::InvalidTableAddress)
        );
    }

    #[test]
    fn ancestor_permissions_restrict_leaf_access() {
        let va = 0x1234_5678_9abcu64;
        let ix = indices(va).unwrap();
        let mut h = hierarchy(va);
        h.tables[1][ix[1]].writable = false;
        assert_eq!(h.translate(va, true, false), Err(WalkError::WriteDenied));
        assert_eq!(h.translate(va, false, false), Ok(0x9abc));
        h.tables[0][ix[0]].user = false;
        assert_eq!(h.translate(va, false, true), Err(WalkError::UserDenied));
    }
    #[test]
    fn write_requires_writable_permission_at_every_level() {
        let va = 0x1234_5678_9abcu64;
        let ix = indices(va).unwrap();
        for (level, slot) in ix.iter().copied().enumerate() {
            let mut h = hierarchy(va);
            h.tables[level][slot].writable = false;
            assert_eq!(h.translate(va, true, false), Err(WalkError::WriteDenied));
            assert_eq!(h.translate(va, false, false), Ok(0x9abc));
        }
    }

    #[test]
    fn every_level_must_be_present_and_permit_user_access() {
        let va = 0x1234_5678_9abcu64;
        let ix = indices(va).unwrap();
        for (level, slot) in ix.iter().copied().enumerate() {
            let mut h = hierarchy(va);
            h.tables[level][slot].present = false;
            assert_eq!(h.translate(va, false, false), Err(WalkError::MissingEntry));
            h = hierarchy(va);
            h.tables[level][slot].user = false;
            assert_eq!(h.translate(va, false, true), Err(WalkError::UserDenied));
        }
    }
}
