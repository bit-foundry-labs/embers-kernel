//! Host-only research harness for ordering paging preflight before a mock writer.
use crate::address_provenance::{AddressProvenance, ProvenanceError};
use crate::frame_registry::{FrameRegistry, FrameReservation, RegistryError};
use crate::paging_mode::{self, PagingModeError};

const MAX_TRANSITION_RESERVATIONS: usize = 16;
const MAX_BLOCKERS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightInput<'a> {
    pub cr0: u64,
    pub cr4: u64,
    pub provenance: Option<AddressProvenance>,
    pub reservations: &'a [FrameReservation],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateError {
    PagingMode(PagingModeError),
    MissingProvenance,
    InvalidProvenance(ProvenanceError),
    MissingReservations,
    InvalidReservation {
        index: usize,
        error: RegistryError,
    },
    InvalidHhdmCandidate {
        index: usize,
        error: ProvenanceError,
    },
    TableConstructionIncomplete,
    StackMappingEvidenceMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreflightReport {
    errors: [Option<GateError>; MAX_BLOCKERS],
    len: usize,
}

impl PreflightReport {
    fn new() -> Self {
        Self {
            errors: [None; MAX_BLOCKERS],
            len: 0,
        }
    }

    fn push(&mut self, error: GateError) {
        self.errors[self.len] = Some(error);
        self.len += 1;
    }

    pub fn is_blocked(&self) -> bool {
        self.len != 0
    }

    pub fn errors(&self) -> impl Iterator<Item = GateError> + '_ {
        self.errors[..self.len].iter().flatten().copied()
    }
}

/// Validate only existing numeric/mode and reservation checks.
///
/// There is no page-table builder or stack/mapping proof API yet, so those two
/// conditions are always reported as blockers; this harness cannot become
/// ready by caller-asserted booleans or fabricated evidence.
pub fn preflight(input: &PreflightInput<'_>) -> PreflightReport {
    let mut report = PreflightReport::new();

    if let Err(error) = paging_mode::validate(input.cr0, input.cr4) {
        report.push(GateError::PagingMode(error));
    }

    let provenance = match input.provenance {
        None => {
            report.push(GateError::MissingProvenance);
            None
        }
        Some(value) => match AddressProvenance::new(
            value.hhdm_offset,
            value.executable_physical_base,
            value.executable_virtual_base,
        ) {
            Ok(validated) => Some(validated),
            Err(error) => {
                report.push(GateError::InvalidProvenance(error));
                None
            }
        },
    };

    let mut registry = FrameRegistry::<MAX_TRANSITION_RESERVATIONS>::new();
    let mut reservation_error = None;
    if input.reservations.is_empty() {
        report.push(GateError::MissingReservations);
        reservation_error = Some(());
    } else {
        for (index, reservation) in input.reservations.iter().enumerate() {
            if let Err(error) =
                registry.reserve(reservation.start, reservation.pages, reservation.owner)
            {
                report.push(GateError::InvalidReservation { index, error });
                reservation_error = Some(());
                break;
            }
        }
    }

    if reservation_error.is_none() {
        if let Some(provenance) = provenance {
            for (index, reservation) in input.reservations.iter().enumerate() {
                let candidate = provenance.hhdm_candidate(reservation.start).and_then(|_| {
                    reservation
                        .pages
                        .checked_sub(1)
                        .and_then(|pages| pages.checked_mul(crate::memory::PAGE_BYTES))
                        .and_then(|offset| reservation.start.checked_add(offset))
                        .ok_or(ProvenanceError::AddressOverflow)
                        .and_then(|last| provenance.hhdm_candidate(last))
                });
                if let Err(error) = candidate {
                    report.push(GateError::InvalidHhdmCandidate { index, error });
                    break;
                }
            }
        }
    }

    report.push(GateError::TableConstructionIncomplete);
    report.push(GateError::StackMappingEvidenceMissing);
    report
}

/// Run a mock-only writer after a fully clear report.
///
/// Current reports always contain the unsupported table and mapping blockers,
/// so this harness deliberately has no successful transition path.
pub fn attempt_mock_transition(
    input: &PreflightInput<'_>,
    mock_writer: impl FnOnce(),
) -> Result<(), PreflightReport> {
    let report = preflight(input);
    if report.is_blocked() {
        return Err(report);
    }
    mock_writer();
    Ok(())
}
