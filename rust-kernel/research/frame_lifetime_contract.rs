//! Host-only research model for physical-frame ownership and lifetime gates.
//!
//! This module cannot grant kernel authority, prove caller-supplied evidence,
//! dereference a frame, construct a page table, reclaim memory, or write CR3.
//! All supplied evidence is HYPOTHETICAL; it is not a trusted runtime API.
#![allow(dead_code)]

const PAGE_BYTES: u64 = 4096;
const MIN_FRAME_ADDRESS: u64 = 0x0010_0000;
const LIMINE_USABLE: u64 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateError {
    InvalidRange,
    NotUsable,
    OverlapsExcludedMemory,
    WrongPhase,
    EvidenceMissing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSpan {
    start: u64,
    pages: u64,
}

impl FrameSpan {
    pub fn new(start: u64, pages: u64) -> Result<Self, GateError> {
        if start < MIN_FRAME_ADDRESS || start % PAGE_BYTES != 0 || pages == 0 {
            return Err(GateError::InvalidRange);
        }
        let size = pages.checked_mul(PAGE_BYTES).ok_or(GateError::InvalidRange)?;
        start.checked_add(size).ok_or(GateError::InvalidRange)?;
        Ok(Self { start, pages })
    }

    fn end(self) -> u64 {
        // Invariant established by the checked constructor.
        self.start + self.pages * PAGE_BYTES
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end() && other.start < self.end()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntendedUse {
    PageTable,
    TransitionStack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Candidate,
    Reserved,
    Prepared,
    Active,
    Retired,
    Quarantined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRecord {
    pub span: FrameSpan,
    pub intended_use: IntendedUse,
    pub phase: Phase,
}

/// Caller-asserted observations for DESIGN TESTS, not unforgeable proofs.
/// Every field requires a future, separately reviewed enforcement/evidence
/// source before it could be considered by a real memory manager.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClaimedEvidence {
    pub sole_issuer_and_no_conflicting_claim: bool,
    pub observed_correct_writable_backing: bool,
    pub exclusive_write_access: bool,
    pub zero_initialization_completed: bool,
    pub live_code_data_and_stack_mappings_preserved: bool,
    pub fault_descriptors_and_ist_mappings_preserved: bool,
    pub old_root_and_boot_storage_pinned: bool,
    pub interrupts_disabled_on_single_bsp: bool,
    pub no_new_references_after_retirement: bool,
    pub cpu_translations_quiesced: bool,
    pub device_translations_quiesced: bool,
    pub outstanding_references_drained: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposedStep {
    Reserve,
    Prepare,
    Activate,
    Retire,
    Quarantine,
}

impl FrameRecord {
    /// Numeric candidate selection excludes known unavailable physical spans.
    /// Limine's classification is trusted external input, NOT a capability.
    pub fn candidate(
        span: FrameSpan,
        limine_kind: u64,
        excluded: &[FrameSpan],
        intended_use: IntendedUse,
    ) -> Result<Self, GateError> {
        if limine_kind != LIMINE_USABLE {
            return Err(GateError::NotUsable);
        }
        if excluded.iter().any(|forbidden| span.overlaps(*forbidden)) {
            return Err(GateError::OverlapsExcludedMemory);
        }
        Ok(Self {
            span,
            intended_use,
            phase: Phase::Candidate,
        })
    }

    /// A host model of required ordering, NOT an authorization to modify RAM.
    /// Failed steps return an error and leave the original record unchanged.
    pub fn advance(
        self,
        step: ProposedStep,
        evidence: ClaimedEvidence,
    ) -> Result<Self, GateError> {
        let (expected, next, sufficient) = match step {
            ProposedStep::Reserve => (
                Phase::Candidate,
                Phase::Reserved,
                evidence.sole_issuer_and_no_conflicting_claim,
            ),
            ProposedStep::Prepare => (
                Phase::Reserved,
                Phase::Prepared,
                evidence.observed_correct_writable_backing
                    && evidence.exclusive_write_access
                    && evidence.zero_initialization_completed,
            ),
            ProposedStep::Activate => (
                Phase::Prepared,
                Phase::Active,
                evidence.live_code_data_and_stack_mappings_preserved
                    && evidence.fault_descriptors_and_ist_mappings_preserved
                    && evidence.old_root_and_boot_storage_pinned
                    && evidence.interrupts_disabled_on_single_bsp,
            ),
            ProposedStep::Retire => (
                Phase::Active,
                Phase::Retired,
                evidence.no_new_references_after_retirement,
            ),
            ProposedStep::Quarantine => (
                Phase::Retired,
                Phase::Quarantined,
                evidence.cpu_translations_quiesced
                    && evidence.device_translations_quiesced
                    && evidence.outstanding_references_drained,
            ),
        };
        if self.phase != expected {
            return Err(GateError::WrongPhase);
        }
        if !sufficient {
            return Err(GateError::EvidenceMissing);
        }
        Ok(Self {
            phase: next,
            ..self
        })
    }
}

// Deliberately NO release-to-free or reinterpret-as-capability operation.
// Even Quarantined cannot be reused under this research model.

#[cfg(test)]
mod tests {
    use super::*;

    fn full_evidence() -> ClaimedEvidence {
        ClaimedEvidence {
            sole_issuer_and_no_conflicting_claim: true,
            observed_correct_writable_backing: true,
            exclusive_write_access: true,
            zero_initialization_completed: true,
            live_code_data_and_stack_mappings_preserved: true,
            fault_descriptors_and_ist_mappings_preserved: true,
            old_root_and_boot_storage_pinned: true,
            interrupts_disabled_on_single_bsp: true,
            no_new_references_after_retirement: true,
            cpu_translations_quiesced: true,
            device_translations_quiesced: true,
            outstanding_references_drained: true,
        }
    }

    fn candidate() -> FrameRecord {
        FrameRecord::candidate(
            FrameSpan::new(0x200000, 2).unwrap(),
            LIMINE_USABLE,
            &[FrameSpan::new(0x100000, 2).unwrap()],
            IntendedUse::PageTable,
        )
        .unwrap()
    }

    #[test]
    fn rejects_bad_span_or_excluded_reserved_memory() {
        for (start, pages) in [
            (0, 1),
            (0x200001, 1),
            (0x200000, 0),
            (0xffff_ffff_ffff_f000, 2),
            (0x200000, u64::MAX),
        ] {
            assert_eq!(FrameSpan::new(start, pages), Err(GateError::InvalidRange));
        }
        let span = FrameSpan::new(0x200000, 2).unwrap();
        assert_eq!(
            FrameRecord::candidate(span, 1, &[], IntendedUse::PageTable),
            Err(GateError::NotUsable)
        );
        assert_eq!(
            FrameRecord::candidate(
                span,
                LIMINE_USABLE,
                &[FrameSpan::new(0x201000, 1).unwrap()],
                IntendedUse::PageTable
            ),
            Err(GateError::OverlapsExcludedMemory)
        );
        assert!(!span.overlaps(FrameSpan::new(0x202000, 1).unwrap()));
    }

    #[test]
    fn rejects_missing_issuer_and_disallows_phase_skips() {
        let record = candidate();
        assert_eq!(
            record.advance(ProposedStep::Reserve, ClaimedEvidence::default()),
            Err(GateError::EvidenceMissing)
        );
        assert_eq!(
            record.advance(ProposedStep::Activate, full_evidence()),
            Err(GateError::WrongPhase)
        );
        assert_eq!(record.phase, Phase::Candidate);
    }

    #[test]
    fn initialization_requires_backing_exclusion_and_zeroing() {
        let reserved = candidate()
            .advance(ProposedStep::Reserve, full_evidence())
            .unwrap();
        for missing in 0..3 {
            let mut evidence = full_evidence();
            match missing {
                0 => evidence.observed_correct_writable_backing = false,
                1 => evidence.exclusive_write_access = false,
                _ => evidence.zero_initialization_completed = false,
            }
            assert_eq!(
                reserved.advance(ProposedStep::Prepare, evidence),
                Err(GateError::EvidenceMissing)
            );
            assert_eq!(reserved.phase, Phase::Reserved);
        }
    }

    #[test]
    fn activation_requires_surviving_code_stacks_descriptors_and_old_root() {
        let ready = candidate()
            .advance(ProposedStep::Reserve, full_evidence())
            .unwrap()
            .advance(ProposedStep::Prepare, full_evidence())
            .unwrap();

        for missing in 0..4 {
            let mut evidence = full_evidence();
            match missing {
                0 => evidence.live_code_data_and_stack_mappings_preserved = false,
                1 => evidence.fault_descriptors_and_ist_mappings_preserved = false,
                2 => evidence.old_root_and_boot_storage_pinned = false,
                _ => evidence.interrupts_disabled_on_single_bsp = false,
            }
            assert_eq!(
                ready.advance(ProposedStep::Activate, evidence),
                Err(GateError::EvidenceMissing)
            );
        }
        assert_eq!(ready.phase, Phase::Prepared);
    }

    #[test]
    fn retirement_and_quarantine_require_separate_quiescence() {
        let mut record = candidate();
        for step in [
            ProposedStep::Reserve,
            ProposedStep::Prepare,
            ProposedStep::Activate,
        ] {
            record = record.advance(step, full_evidence()).unwrap();
        }
        assert_eq!(record.phase, Phase::Active);
        assert_eq!(
            record.advance(ProposedStep::Retire, ClaimedEvidence::default()),
            Err(GateError::EvidenceMissing)
        );
        record = record.advance(ProposedStep::Retire, full_evidence()).unwrap();
        assert_eq!(record.phase, Phase::Retired);

        for missing in 0..3 {
            let mut evidence = full_evidence();
            match missing {
                0 => evidence.cpu_translations_quiesced = false,
                1 => evidence.device_translations_quiesced = false,
                _ => evidence.outstanding_references_drained = false,
            }
            assert_eq!(
                record.advance(ProposedStep::Quarantine, evidence),
                Err(GateError::EvidenceMissing)
            );
        }
        record = record.advance(ProposedStep::Quarantine, full_evidence()).unwrap();
        assert_eq!(record.phase, Phase::Quarantined);
        for step in [
            ProposedStep::Reserve,
            ProposedStep::Prepare,
            ProposedStep::Activate,
            ProposedStep::Retire,
            ProposedStep::Quarantine,
        ] {
            assert_eq!(record.advance(step, full_evidence()), Err(GateError::WrongPhase));
        }
    }

    #[test]
    fn stack_role_uses_same_conservative_lifetime_contract() {
        let stack = FrameRecord::candidate(
            FrameSpan::new(0x300000, 4).unwrap(),
            LIMINE_USABLE,
            &[],
            IntendedUse::TransitionStack,
        )
        .unwrap();
        let reserved = stack.advance(ProposedStep::Reserve, full_evidence()).unwrap();
        assert_eq!(reserved.intended_use, IntendedUse::TransitionStack);
        assert_eq!(reserved.phase, Phase::Reserved);
    }
}

fn main() {}
