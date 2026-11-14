// Host tests cover control-flow ordering only; no privileged register is written.
#[path = "../src/address_provenance.rs"]
mod address_provenance;
#[path = "../research/cr3_transition_gate.rs"]
mod cr3_transition_gate;
#[path = "../src/frame_registry.rs"]
mod frame_registry;
#[path = "../src/memory.rs"]
mod memory;
#[path = "../src/paging_mode.rs"]
mod paging_mode;

use address_provenance::AddressProvenance;
use cr3_transition_gate::{attempt_mock_transition, GateError, PreflightInput};
use frame_registry::{FrameReservation, RegistryError};
use memory::PAGE_BYTES;
use paging_mode::PagingModeError;

const CR0_PG: u64 = 1 << 31;
const CR4_PAE: u64 = 1 << 5;
const CR4_LA57: u64 = 1 << 12;
const OLD_ROOT: u64 = 0x1234_5000;
const NEW_ROOT: u64 = 0x5678_9000;

fn provenance() -> AddressProvenance {
    AddressProvenance::new(0xffff_8000_0000_0000, 0x200000, 0xffff_ffff_8000_0000).unwrap()
}

fn reservation(start: u64, pages: u64, owner: u32) -> FrameReservation {
    FrameReservation {
        start,
        pages,
        owner,
    }
}

fn valid_input<'a>(reservations: &'a [FrameReservation]) -> PreflightInput<'a> {
    PreflightInput {
        cr0: CR0_PG,
        cr4: CR4_PAE,
        provenance: Some(provenance()),
        reservations,
    }
}

fn assert_rejected_without_write(input: &PreflightInput<'_>, expected: GateError) {
    let mut active_root = OLD_ROOT;
    let mut writer_calls = 0;
    let result = attempt_mock_transition(input, || {
        writer_calls += 1;
        active_root = NEW_ROOT;
    });

    let report = result.expect_err("the unsupported transition must remain blocked");
    assert!(report.errors().any(|error| error == expected));
    assert_eq!(writer_calls, 0);
    assert_eq!(active_root, OLD_ROOT);
}

#[test]
fn rejects_each_unsupported_paging_mode_without_mock_write() {
    let reservations = [reservation(0x100000, 1, 1)];
    for (cr0, cr4, expected) in [
        (
            0,
            CR4_PAE,
            GateError::PagingMode(PagingModeError::PagingDisabled),
        ),
        (
            CR0_PG,
            0,
            GateError::PagingMode(PagingModeError::PhysicalAddressExtensionDisabled),
        ),
        (
            CR0_PG,
            CR4_PAE | CR4_LA57,
            GateError::PagingMode(PagingModeError::FiveLevelPagingUnsupported),
        ),
    ] {
        let input = PreflightInput {
            cr0,
            cr4,
            ..valid_input(&reservations)
        };
        assert_rejected_without_write(&input, expected);
    }
}

#[test]
fn rejects_missing_and_invalid_provenance_without_mock_write() {
    let reservations = [reservation(0x100000, 1, 1)];
    let missing = PreflightInput {
        provenance: None,
        ..valid_input(&reservations)
    };
    assert_rejected_without_write(&missing, GateError::MissingProvenance);

    let invalid = PreflightInput {
        provenance: Some(AddressProvenance {
            hhdm_offset: 0,
            executable_physical_base: 0x200000,
            executable_virtual_base: 0xffff_ffff_8000_0000,
        }),
        ..valid_input(&reservations)
    };
    assert_rejected_without_write(
        &invalid,
        GateError::InvalidProvenance(address_provenance::ProvenanceError::InvalidDirectMapOffset),
    );
}

#[test]
fn rejects_missing_invalid_and_overlapping_reservations_without_mock_write() {
    let missing: [FrameReservation; 0] = [];
    assert_rejected_without_write(&valid_input(&missing), GateError::MissingReservations);

    let invalid = [reservation(0x100000, 0, 1)];
    assert_rejected_without_write(
        &valid_input(&invalid),
        GateError::InvalidReservation {
            index: 0,
            error: RegistryError::ZeroPages,
        },
    );

    let overlapping = [reservation(0x100000, 2, 1), reservation(0x101000, 1, 2)];
    assert_rejected_without_write(
        &valid_input(&overlapping),
        GateError::InvalidReservation {
            index: 1,
            error: RegistryError::Overlap,
        },
    );
}

#[test]
fn unsupported_table_and_stack_evidence_are_explicit_ordered_blockers() {
    let reservations = [reservation(0x100000, 1, 1)];
    let report = cr3_transition_gate::preflight(&valid_input(&reservations));
    assert_eq!(
        report.errors().collect::<Vec<_>>(),
        [
            GateError::TableConstructionIncomplete,
            GateError::StackMappingEvidenceMissing,
        ]
    );
    assert_rejected_without_write(
        &valid_input(&reservations),
        GateError::TableConstructionIncomplete,
    );
    assert_rejected_without_write(
        &valid_input(&reservations),
        GateError::StackMappingEvidenceMissing,
    );
}

#[test]
fn error_order_is_stable_when_multiple_preconditions_fail() {
    let overlapping = [reservation(0x100000, 2, 1), reservation(0x101000, 1, 2)];
    let input = PreflightInput {
        cr0: 0,
        cr4: 0,
        provenance: None,
        reservations: &overlapping,
    };
    let report = cr3_transition_gate::preflight(&input);
    assert_eq!(
        report.errors().collect::<Vec<_>>(),
        [
            GateError::PagingMode(PagingModeError::PagingDisabled),
            GateError::MissingProvenance,
            GateError::InvalidReservation {
                index: 1,
                error: RegistryError::Overlap,
            },
            GateError::TableConstructionIncomplete,
            GateError::StackMappingEvidenceMissing,
        ]
    );
}

#[test]
fn reservation_range_overflow_is_rejected_without_mock_write() {
    let reservations = [reservation(u64::MAX & !(PAGE_BYTES - 1), 2, 1)];
    assert_rejected_without_write(
        &valid_input(&reservations),
        GateError::InvalidReservation {
            index: 0,
            error: RegistryError::AddressOverflow,
        },
    );
}

#[test]
fn hhdm_candidate_overflow_is_rejected_without_mock_write() {
    let reservations = [reservation(0x100000, 1, 1)];
    let input = PreflightInput {
        provenance: Some(
            AddressProvenance::new(0xffff_ffff_ffff_f000, 0x200000, 0xffff_ffff_8000_0000).unwrap(),
        ),
        ..valid_input(&reservations)
    };
    assert_rejected_without_write(
        &input,
        GateError::InvalidHhdmCandidate {
            index: 0,
            error: address_provenance::ProvenanceError::AddressOverflow,
        },
    );
}
