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

// A host-only staged batch model, not an actual page-table constructor.
// The real M06 transition is blocked: this never touches frames, CPU page
// tables, the active root, or a CR3 writer.
#[derive(Clone, Copy)]
struct ProposedLeaf {
    slot: usize,
    frame: u64,
    exclusive: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum StageError {
    Injected(usize),
    ConflictingExclusiveAlias,
    Leaf(page_entry::TableError),
}

// This model intentionally retains earlier staged entries after an error.
// A future builder must keep partially constructed tables uninstalled and
// establish physical-frame ownership before writing real memory.
fn stage_candidate_leaves(
    table: &mut LeafTable,
    plan: &[ProposedLeaf],
    fail_before: Option<usize>,
) -> Result<(), StageError> {
    for (index, mapping) in plan.iter().enumerate() {
        if fail_before == Some(index) {
            return Err(StageError::Injected(index));
        }
        if plan[..index]
            .iter()
            .any(|prior| prior.frame == mapping.frame && (prior.exclusive || mapping.exclusive))
        {
            return Err(StageError::ConflictingExclusiveAlias);
        }
        table
            .insert(mapping.slot, mapping.frame, 36, true)
            .map_err(StageError::Leaf)?;
    }
    if fail_before == Some(plan.len()) {
        return Err(StageError::Injected(plan.len()));
    }
    Ok(())
}

fn proposed_leaves() -> [ProposedLeaf; 3] {
    [
        ProposedLeaf {
            slot: 2,
            frame: 0x4000,
            exclusive: true,
        },
        ProposedLeaf {
            slot: 3,
            frame: 0x5000,
            exclusive: true,
        },
        ProposedLeaf {
            slot: 4,
            frame: 0x6000,
            exclusive: true,
        },
    ]
}

#[test]
fn every_injected_construction_boundary_leaves_only_a_staged_prefix() {
    let plan = proposed_leaves();
    for completed in 0..=plan.len() {
        let mut candidate = LeafTable::new();
        assert_eq!(
            stage_candidate_leaves(&mut candidate, &plan, Some(completed)),
            Err(StageError::Injected(completed))
        );
        for (index, mapping) in plan.iter().enumerate() {
            let expected = if index < completed {
                mapping.frame | 0b11
            } else {
                0
            };
            assert_eq!(candidate.entry(mapping.slot), Some(expected));
        }
    }
}

#[test]
fn late_leaf_validation_error_preserves_partial_candidate_for_detection() {
    let mut plan = proposed_leaves();
    plan[2].frame = 0x6001;
    let mut candidate = LeafTable::new();
    assert_eq!(
        stage_candidate_leaves(&mut candidate, &plan, None),
        Err(StageError::Leaf(page_entry::TableError::InvalidEntry(
            page_entry::EntryError::UnalignedAddress
        )))
    );
    assert_eq!(candidate.entry(plan[0].slot), Some(0x4003));
    assert_eq!(candidate.entry(plan[1].slot), Some(0x5003));
    assert_eq!(candidate.entry(plan[2].slot), Some(0));
}

#[test]
fn exclusive_alias_is_rejected_without_banning_all_intentional_aliases() {
    let mut plan = proposed_leaves();
    plan[1].frame = plan[0].frame;

    let mut candidate = LeafTable::new();
    assert_eq!(
        stage_candidate_leaves(&mut candidate, &plan, None),
        Err(StageError::ConflictingExclusiveAlias)
    );
    assert_eq!(candidate.entry(plan[0].slot), Some(0x4003));
    assert_eq!(candidate.entry(plan[1].slot), Some(0));

    // An explicitly non-exclusive host model can represent two aliases.
    // This is not an authorization or proof that such aliases are safe.
    plan[0].exclusive = false;
    plan[1].exclusive = false;
    let mut shared_candidate = LeafTable::new();
    assert_eq!(
        stage_candidate_leaves(&mut shared_candidate, &plan, None),
        Ok(())
    );
    assert_eq!(shared_candidate.entry(plan[0].slot), Some(0x4003));
    assert_eq!(shared_candidate.entry(plan[1].slot), Some(0x4003));
}
