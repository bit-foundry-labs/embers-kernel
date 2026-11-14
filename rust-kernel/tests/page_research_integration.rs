//! Connects the host-only leaf encoder to the synthetic page-walk model.
//! The explicit decoding below is test scaffolding, not a hardware decoder.

#[path = "../research/page_entry.rs"]
mod page_entry;
#[path = "../research/page_walk.rs"]
mod page_walk;

use page_entry::{supervisor_leaf, LeafTable};
use page_walk::{indices, Entry, Hierarchy, WalkError};

const EMPTY: Entry = Entry {
    target: 0,
    present: false,
    writable: false,
    user: false,
};

fn hierarchy(va: u64, leaf: Entry) -> Hierarchy {
    let mut h = Hierarchy {
        table_addresses: [0x1000, 0x2000, 0x3000, 0x4000],
        tables: [[EMPTY; 512]; 4],
    };
    let slots = indices(va).unwrap();
    for (level, slot) in slots.iter().copied().enumerate().take(3) {
        h.tables[level][slot] = Entry {
            target: h.table_addresses[level + 1],
            present: true,
            writable: true,
            user: true,
        };
    }
    h.tables[3][slots[3]] = leaf;
    h
}

#[test]
fn encoded_supervisor_leaf_drives_translation_and_permissions() {
    let va = 0x1234_5678_9abcu64;
    let address = 0x1234_5000u64;
    for writable in [false, true] {
        let slot = indices(va).unwrap()[3];
        let mut table = LeafTable::new();
        table.insert(slot, address, 36, writable).unwrap();
        let encoded = table.entry(slot).unwrap();
        assert_eq!(encoded, supervisor_leaf(address, 36, writable).unwrap());
        // The encoder supports only present, writable, and physical-address bits.
        assert_eq!(encoded & 0xffc, 0);
        let leaf = Entry {
            target: encoded & !0xfff,
            present: encoded & 1 != 0,
            writable: encoded & 2 != 0,
            user: encoded & 4 != 0,
        };
        let h = hierarchy(va, leaf);
        assert_eq!(h.translate(va, false, false), Ok(address + 0xabc));
        assert_eq!(h.translate(va, false, true), Err(WalkError::UserDenied));
        let write = h.translate(va, true, false);
        if writable {
            assert_eq!(write, Ok(address + 0xabc));
        } else {
            assert_eq!(write, Err(WalkError::WriteDenied));
        }
    }
}
