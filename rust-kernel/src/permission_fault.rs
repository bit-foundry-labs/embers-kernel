//! Fatal, feature-gated QEMU experiments for inherited paging permissions.
//!
//! These statics are in distinct ELF PT_LOAD protection classes. No page
//! table, CPU control register, descriptor, or handler is modified.
//! Neither target is accessed during normal kernel boot.

use core::arch::asm;
use core::ptr::addr_of;

const PAGE_BYTES: usize = 4096;

#[repr(align(4096))]
struct FaultPage([u8; PAGE_BYTES]);

// An isolated page in the ELF read-only PT_LOAD segment. The test tries
// one byte store while CPL0 CR0.WP is enabled. This must fault, not write.
#[cfg(feature = "fault-ro-write")]
#[used]
#[link_section = ".rodata.permission_fault"]
static READ_ONLY_TARGET: FaultPage = FaultPage([0xa5; PAGE_BYTES]);

// An isolated page in the ELF writable, NX PT_LOAD segment. If execution
// is accidentally allowed, the first bytes are UD2 (an unexpected #UD),
// making the test fail rather than proceeding through arbitrary data.
#[cfg(feature = "fault-nx-exec")]
const fn nx_canary() -> FaultPage {
    let mut bytes = [0; PAGE_BYTES];
    bytes[0] = 0x0f;
    bytes[1] = 0x0b;
    FaultPage(bytes)
}

#[cfg(feature = "fault-nx-exec")]
#[used]
#[link_section = ".data.permission_fault"]
static NON_EXECUTABLE_TARGET: FaultPage = nx_canary();

/// Return the intended supervisor write-fault target for serial correlation.
#[cfg(feature = "fault-ro-write")]
pub fn read_only_target() -> u64 {
    addr_of!(READ_ONLY_TARGET.0) as u64
}

/// Return the intended instruction-fetch-fault target for serial correlation.
#[cfg(feature = "fault-nx-exec")]
pub fn non_executable_target() -> u64 {
    addr_of!(NON_EXECUTABLE_TARGET.0) as u64
}

/// # Safety
/// Call only on a single CPL0 BSP in an isolated feature-only QEMU boot
/// after CR0.WP and the read-only ELF mapping have been checked. If WP
/// unexpectedly fails, this writes one byte in the private canary page.
#[cfg(feature = "fault-ro-write")]
pub unsafe fn trigger_supervisor_write() {
    // SAFETY: Deliberately attempt a one-byte write to the private static.
    // The test requires a fatal supervisor write-protection #PF.
    unsafe {
        asm!(
            "mov byte ptr [{target}], 0",
            target = in(reg) read_only_target(),
            options(nostack, preserves_flags)
        );
    }
}

/// # Safety
/// Call only from an isolated feature-only CPL0 QEMU boot after EFER.NXE
/// and the NX-mapped ELF data segment have been observed. The inline jump
/// never returns; a return to the caller indicates an invalid test path.
#[cfg(feature = "fault-nx-exec")]
pub unsafe fn trigger_instruction_fetch() {
    // SAFETY: Deliberately fetch from the private RW/NX data page. If NX
    // is absent, the first two bytes cause #UD rather than continuing.
    unsafe {
        asm!(
            "jmp {target}",
            target = in(reg) non_executable_target(),
            options(noreturn)
        )
    }
}
