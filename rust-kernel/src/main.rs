//! Minimal Embers x86-64 Rust kernel entry. Not a protected microkernel.
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

// The live stack switch is deliberately isolated from other fault and
// stack experiments; normal boot and paging state remain unchanged.
#[cfg(all(
    feature = "transition-stack-switch",
    any(
        feature = "transition-stack-smoke",
        feature = "boot-ownership-smoke",
        feature = "fault-ud",
        feature = "fault-pf",
        feature = "fault-df",
        feature = "fault-ro-write",
        feature = "fault-nx-exec"
    )
))]
compile_error!("transition-stack-switch cannot be combined with other test features.");

// Feature-only stack smoke is separate from fatal fault-injection runs.
#[cfg(all(
    feature = "transition-stack-smoke",
    any(
        feature = "fault-ud",
        feature = "fault-pf",
        feature = "fault-df",
        feature = "fault-ro-write",
        feature = "fault-nx-exec"
    )
))]
compile_error!("transition-stack-smoke cannot be combined with fault injection modes.");

// Each fault-injection feature deliberately terminates boot with its fault.
// Combining modes would make the observed fault depend on execution order.
#[cfg(any(
    all(feature = "fault-ud", feature = "fault-pf"),
    all(feature = "fault-ud", feature = "fault-df"),
    all(feature = "fault-pf", feature = "fault-df")
))]
compile_error!(
    "QEMU fault-injection features are mutually exclusive: choose only one of fault-ud, fault-pf, or fault-df."
);

#[cfg(any(
    all(
        feature = "fault-ro-write",
        any(
            feature = "fault-ud",
            feature = "fault-pf",
            feature = "fault-df",
            feature = "fault-nx-exec"
        )
    ),
    all(
        feature = "fault-nx-exec",
        any(feature = "fault-ud", feature = "fault-pf", feature = "fault-df")
    )
))]
compile_error!("QEMU permission fault tests are mutually exclusive with other fault modes.");

mod address_provenance;
mod arch;
mod boot_addresses;
mod boot_memory;
#[cfg(feature = "boot-ownership-smoke")]
mod frame_ledger;
mod frame_registry;
mod kernel_state;
mod memory;
mod paging_mode;
#[cfg(any(feature = "fault-ro-write", feature = "fault-nx-exec"))]
mod permission_fault;
mod serial;
#[cfg(any(
    feature = "transition-stack-smoke",
    feature = "transition-stack-switch"
))]
mod transition_stack;

use core::arch::asm;
use core::panic::PanicInfo;

const COM1: u16 = 0x3f8;
const UART_TRANSMIT_READY: u8 = 1 << 5;
const UART_MAX_POLLS: usize = 100_000;

// Limine protocol base revision 6; matches the independent C boot reference.
// https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md
#[used]
#[link_section = ".limine_requests_start"]
static REQUESTS_START: [u64; 4] = [
    0xf6b8f4b39de7d1ae,
    0xfab91a6940fcb9cf,
    0x785c6ed015d3e316,
    0x181e920a7852b9d9,
];

#[used]
#[link_section = ".limine_requests"]
static mut BASE_REVISION: [u64; 3] = [0xf9562b2d5c95a6c8, 0x6a7b384944536bdc, 6];

#[used]
#[link_section = ".limine_requests_end"]
static REQUESTS_END: [u64; 2] = [0xadc0e0531bb10d03, 0x9572709f31764c62];

/// # Safety
/// Requires privileged x86-64 execution with I/O access to COM1.
#[inline]
unsafe fn port_write(port: u16, value: u8) {
    // SAFETY: Caller guarantees privilege and port access.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
    }
}

/// # Safety
/// Requires privileged x86-64 execution with I/O access to COM1.
#[inline]
unsafe fn port_read(port: u16) -> u8 {
    let value: u8;
    // SAFETY: Caller guarantees privilege and port access.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nostack, preserves_flags));
    }
    value
}

fn serial_init() {
    // SAFETY: Limine's handoff runs this kernel in ring 0. The QEMU
    // smoke test provides an emulated legacy COM1 UART.
    unsafe {
        port_write(COM1 + 1, 0x00); // Disable UART interrupts.
        port_write(COM1 + 3, 0x80); // Divisor latch access.
        port_write(COM1, 0x03); // 38400 baud, 115200 base clock.
        port_write(COM1 + 1, 0x00);
        port_write(COM1 + 3, 0x03); // 8N1.
        port_write(COM1 + 2, 0xc7); // Enable and clear FIFOs.
        port_write(COM1 + 4, 0x0b); // DTR, RTS, OUT2.
    }
}

fn serial_write_byte(byte: u8) -> bool {
    for _ in 0..UART_MAX_POLLS {
        // SAFETY: Only called from the privileged entry/panic paths.
        if unsafe { port_read(COM1 + 5) } & UART_TRANSMIT_READY != 0 {
            // SAFETY: The UART reported a ready transmit register.
            unsafe { port_write(COM1, byte) };
            return true;
        }
    }
    false
}

/// Emit an entire diagnostic or stop execution if the UART cannot accept
/// a byte. Do not attempt a recursive serial error report from this path.
fn serial_write(message: &str) {
    let result = serial::write_message(message, |byte| {
        if serial_write_byte(byte) {
            Ok(())
        } else {
            Err(serial::SerialError::TransmitTimeout)
        }
    });
    if result.is_err() {
        halt_forever();
    }
}

fn limine_revision_supported() -> bool {
    // SAFETY: Limine may rewrite word 2 before handoff; no other writer
    // is permitted after entry. The volatile read observes its response.
    let revision = unsafe {
        core::ptr::read_volatile(core::ptr::addr_of!(BASE_REVISION).cast::<u64>().add(2))
    };
    revision == 0
}

#[no_mangle]
pub extern "C" fn kernel_entry() -> ! {
    serial_init();
    serial_write("EMBERS: Rust no_std / x86_64 / boot M02\n");

    if !limine_revision_supported() {
        serial_write("ERROR: Limine base revision 6 unsupported\n");
        halt_forever();
    }

    serial_write("BOOT: Limine base revision accepted\n");
    // SAFETY: This is the single BSP at CPL0, with interrupts still disabled.
    unsafe { arch::install() };
    serial_write("BOOT: x86-64 GDT/TSS/IDT installed\n");
    match paging_mode::check_active() {
        Ok(()) => serial_write("PAGING: active four-level mode validated\n"),
        Err(paging_mode::PagingModeError::PagingDisabled) => {
            serial_write("ERROR: CR0.PG disabled\n");
            halt_forever()
        }
        Err(paging_mode::PagingModeError::PhysicalAddressExtensionDisabled) => {
            serial_write("ERROR: CR4.PAE disabled\n");
            halt_forever()
        }
        Err(paging_mode::PagingModeError::FiveLevelPagingUnsupported) => {
            serial_write("ERROR: CR4.LA57 five-level paging unsupported\n");
            halt_forever()
        }
    }
    let protection = match paging_mode::sample_protection_state() {
        Ok(protection) => protection,
        Err(reason) => {
            serial_write("ERROR: CPU paging protection preflight: ");
            serial_write(match reason {
                paging_mode::ProtectionError::MissingExtendedCpuid => "required CPUID leaf missing",
                paging_mode::ProtectionError::InvalidPhysicalAddressWidth => "invalid MAXPHYADDR",
                paging_mode::ProtectionError::InvalidLinearAddressWidth => "invalid linear width",
                paging_mode::ProtectionError::NxUnsupported => "NX unsupported",
                paging_mode::ProtectionError::WriteProtectDisabled => "CR0.WP disabled",
                paging_mode::ProtectionError::NxDisabled => "EFER.NXE disabled",
                paging_mode::ProtectionError::LongModeInactive => "EFER.LMA inactive",
            });
            serial_write("\n");
            halt_forever();
        }
    };
    // These are numeric observations under Limine's active mappings, not
    // verified memory translations or evidence that a new stack can survive CR3.
    let context = paging_mode::sample_active_context();
    if !context.rsp_is_canonical_48() {
        serial_write("ERROR: observed BSP RSP not 48-bit canonical\n");
        halt_forever();
    }
    if !paging_mode::root_candidate_fits_width(
        context.root_frame_candidate(),
        protection.physical_address_bits,
    ) {
        serial_write("ERROR: CR3 root candidate violates MAXPHYADDR\n");
        halt_forever();
    }
    serial_write("PAGING: inherited CR3 raw = ");
    serial_write_hex(context.raw_cr3);
    serial_write("\n");
    serial_write("PAGING: root frame candidate = ");
    serial_write_hex(context.root_frame_candidate());
    serial_write("\n");
    serial_write("PAGING: observed BSP RSP = ");
    serial_write_hex(context.observed_rsp);
    serial_write("\n");
    serial_write("PAGING: MAXPHYADDR bits = ");
    serial_write_hex(protection.physical_address_bits as u64);
    serial_write("\n");
    serial_write("PAGING: maximum linear bits = ");
    serial_write_hex(protection.linear_address_bits as u64);
    serial_write("\n");
    serial_write("PAGING: CPU protection controls active (CR0.WP, EFER.NXE, EFER.LMA)\n");
    let state = kernel_state::KernelState::new(check_boot_memory());
    run_kernel(state)
}

fn run_kernel(state: kernel_state::KernelState<16>) -> ! {
    if state.reservation(0).is_none() {
        serial_write("ERROR: boot reservation missing after initialization\n");
        halt_forever();
    }
    serial_write("MEMORY: boot reservation retained after initialization\n");

    #[cfg(feature = "transition-stack-smoke")]
    {
        // SAFETY: Feature-only QEMU probe on the BSP, interrupts disabled,
        // while Limine's original writable kernel-image mappings remain live.
        let base = match unsafe { transition_stack::probe_inherited_mappings() } {
            Ok(base) => base,
            Err(_) => {
                serial_write("ERROR: transition stack page probe failed\n");
                halt_forever();
            }
        };
        // Feature-only research diagnostic; never include the address in
        // a production boot log or treat it as frame ownership evidence.
        serial_write("STACK: static BSS virtual base = ");
        serial_write_hex(base);
        serial_write("\n");
        serial_write("STACK: four aligned BSS pages writable under inherited mappings\n");
    }

    #[cfg(feature = "transition-stack-switch")]
    {
        // SAFETY: Single-BSP, interrupts-disabled feature-only test after
        // inherited four-level paging and boot-memory preflight succeeded.
        let (base, callback_rsp) = match unsafe { transition_stack::probe_stack_switch() } {
            Ok(result) => result,
            Err(_) => {
                serial_write("ERROR: dedicated stack switch or restore failed\n");
                halt_forever()
            }
        };
        // Reuse the complete four-page QMP mapping observer for this ELF.
        serial_write("STACK: static BSS virtual base = ");
        serial_write_hex(base);
        serial_write("\n");
        serial_write("STACK: four aligned BSS pages writable under inherited mappings\n");
        serial_write("STACK-SWITCH: callback RSP = ");
        serial_write_hex(callback_rsp);
        serial_write("\n");
        serial_write("STACK-SWITCH: original BSP RSP restored\n");
    }

    #[cfg(feature = "fault-ud")]
    {
        serial_write("TEST: intentional invalid opcode\n");
        // SAFETY: Deliberate exception in isolated QEMU testing only.
        unsafe { asm!("ud2", options(nomem, nostack)) };
        serial_write("ERROR: #UD test returned unexpectedly\n");
    }

    #[cfg(feature = "fault-pf")]
    {
        serial_write("TEST: intentional page fault\n");
        // SAFETY: Deliberate exception in isolated QEMU testing only.
        unsafe { arch::trigger_page_fault() };
        serial_write("ERROR: #PF test returned unexpectedly\n");
    }

    #[cfg(feature = "fault-df")]
    {
        serial_write("TEST: intentional double fault through blocked #PF gate\n");
        // SAFETY: Disable #PF delivery in the QEMU test to induce a #DF.
        unsafe {
            arch::remove_page_fault_gate_for_test();
            arch::trigger_page_fault();
        }
        serial_write("ERROR: #DF test returned unexpectedly\n");
    }

    #[cfg(feature = "fault-ro-write")]
    {
        serial_write("TEST: supervisor read-only write target = ");
        serial_write_hex(permission_fault::read_only_target());
        serial_write("\n");
        // SAFETY: Single-BSP feature-only QEMU test; the permission preflight
        // passed, and the target is a private read-only ELF canary page.
        unsafe { permission_fault::trigger_supervisor_write() };
        serial_write("ERROR: read-only supervisor write unexpectedly succeeded\n");
    }

    #[cfg(feature = "fault-nx-exec")]
    {
        serial_write("TEST: supervisor NX fetch target = ");
        serial_write_hex(permission_fault::non_executable_target());
        serial_write("\n");
        // SAFETY: Feature-only QEMU test of a private ELF data page. A
        // failed NX protection must fault via the embedded UD2 canary.
        unsafe { permission_fault::trigger_instruction_fetch() };
        serial_write("ERROR: NX instruction fetch test returned unexpectedly\n");
    }

    #[cfg(not(any(
        feature = "fault-ud",
        feature = "fault-pf",
        feature = "fault-df",
        feature = "fault-ro-write",
        feature = "fault-nx-exec"
    )))]
    serial_write("READY: Embers Rust no_std entry reached.\n");
    halt_forever()
}

fn check_boot_memory() -> frame_registry::TransactionalFrameAllocator<16> {
    let pool = match boot_memory::initial_frame_pool() {
        Ok(pool) => pool,
        Err(boot_memory::BootMapError::AlreadyIssued) => {
            serial_write("ERROR: boot frame pool already issued\n");
            halt_forever()
        }
        Err(boot_memory::BootMapError::NoResponse) => {
            serial_write("ERROR: Limine memory map response missing\n");
            halt_forever()
        }
        Err(boot_memory::BootMapError::InvalidCount) => {
            serial_write("ERROR: memory map entry count invalid or too large\n");
            halt_forever()
        }
        Err(boot_memory::BootMapError::MissingEntry) => {
            serial_write("ERROR: memory map contains null entry\n");
            halt_forever()
        }
        Err(boot_memory::BootMapError::InvalidMap(reason)) => {
            serial_write("ERROR: memory map rejected: ");
            let description = match reason {
                memory::MapError::EmptyMap => "empty",
                memory::MapError::ZeroLength => "zero-length region",
                memory::MapError::AddressOverflow => "physical address overflow",
                memory::MapError::OverlappingRegions => "overlapping regions",
                memory::MapError::InvalidMinimum => "invalid minimum",
                memory::MapError::NoUsableSpan => "no sufficiently large usable span",
            };
            serial_write(description);
            serial_write("\n");
            halt_forever()
        }
    };
    // Exercise the actual boot issuance path before handing the pool onward.
    // This software gate does not establish hardware frame ownership.
    if !matches!(
        boot_memory::initial_frame_pool(),
        Err(boot_memory::BootMapError::AlreadyIssued)
    ) {
        serial_write("ERROR: second boot frame pool issuance was not rejected\n");
        halt_forever();
    }
    // The initialized allocator is returned to the boot owner below.
    #[allow(unused_mut)] // Mutable only when the optional ownership smoke test is enabled.
    let mut allocator =
        match frame_registry::TransactionalFrameAllocator::<16>::with_boot_reservation(pool) {
            Ok(allocator) => allocator,
            Err(_) => {
                serial_write("ERROR: initial frame reservation failed\n");
                halt_forever()
            }
        };
    let Some(initial) = allocator.reservation(0) else {
        serial_write("ERROR: initial reservation missing\n");
        halt_forever()
    };
    let Some(second) = initial.start.checked_add(memory::PAGE_BYTES) else {
        serial_write("ERROR: initial reservation invariant failed\n");
        halt_forever()
    };
    if !second.is_multiple_of(memory::PAGE_BYTES) {
        serial_write("ERROR: initial reservation invariant failed\n");
        halt_forever()
    }
    let first = initial.start;
    // These are address *numbers* and a Limine trust contract, not mappings
    // created or verified by Embers. The selected frames are USABLE map entries.
    let provenance = match boot_addresses::load() {
        Ok(provenance) => provenance,
        Err(boot_addresses::BootAddressError::MissingHhdmResponse) => {
            serial_write("ERROR: Limine HHDM response missing\n");
            halt_forever()
        }
        Err(boot_addresses::BootAddressError::MissingExecutableResponse) => {
            serial_write("ERROR: Limine executable address response missing\n");
            halt_forever()
        }
        Err(boot_addresses::BootAddressError::InvalidMetadata(reason)) => {
            serial_write("ERROR: Limine address metadata invalid: ");
            let label = match reason {
                address_provenance::ProvenanceError::InvalidDirectMapOffset => "HHDM offset",
                address_provenance::ProvenanceError::InvalidExecutableBase => "executable base",
                address_provenance::ProvenanceError::UnalignedFrame => "unaligned frame",
                address_provenance::ProvenanceError::AddressOverflow => "address overflow",
                address_provenance::ProvenanceError::NonCanonicalAddress => "noncanonical address",
            };
            serial_write(label);
            serial_write("\n");
            halt_forever()
        }
    };
    if provenance.hhdm_candidate(first).is_err() || provenance.hhdm_candidate(second).is_err() {
        serial_write("ERROR: frame HHDM address calculation failed\n");
        halt_forever()
    }
    serial_write("ADDRESS: Limine HHDM and executable bases validated\n");
    serial_write("ADDRESS: HHDM offset = ");
    serial_write_hex(provenance.hhdm_offset);
    serial_write("\n");
    serial_write("ADDRESS: executable physical base = ");
    serial_write_hex(provenance.executable_physical_base);
    serial_write("\n");
    serial_write("ADDRESS: executable virtual base = ");
    serial_write_hex(provenance.executable_virtual_base);
    serial_write("\n");
    serial_write("MEMORY: validated Limine map for physical frame accounting\n");
    serial_write("MEMORY: frame claim 1 = ");
    serial_write_hex(first);
    serial_write("\n");
    serial_write("MEMORY: frame claim 2 = ");
    serial_write_hex(second);
    serial_write("\n");
    serial_write("MEMORY: remaining pages in selected span = ");
    serial_write_hex(allocator.remaining_pages());
    serial_write("\n");

    // The normal boot checks frame accounting without running test scenarios.
    #[cfg(feature = "boot-ownership-smoke")]
    run_ownership_smoke(allocator.pool_mut());

    allocator
}

#[cfg(feature = "boot-ownership-smoke")]
fn run_ownership_smoke(pool: &mut memory::FramePool) {
    // This BSP-only test does not control CPU or IOMMU memory mappings.
    let mut ledger = match frame_ledger::FrameLedger::<8>::from_pool(pool) {
        Ok(ledger) => ledger,
        Err(_) => {
            serial_write("ERROR: frame ownership ledger initialization failed\n");
            halt_forever()
        }
    };
    let Some(kernel_owner) = frame_ledger::OwnerId::new(1) else {
        halt_forever()
    };
    let Some(service_owner) = frame_ledger::OwnerId::new(2) else {
        halt_forever()
    };
    let first = match ledger.acquire(kernel_owner) {
        Ok(token) => token,
        Err(_) => {
            serial_write("ERROR: kernel frame ownership claim failed\n");
            halt_forever()
        }
    };
    let second = match ledger.acquire(service_owner) {
        Ok(token) => token,
        Err(_) => {
            serial_write("ERROR: service frame ownership claim failed\n");
            halt_forever()
        }
    };
    if ledger.physical_address(first, service_owner) != Err(frame_ledger::LedgerError::WrongOwner) {
        serial_write("ERROR: cross-owner frame access was accepted\n");
        halt_forever();
    }
    if ledger.quarantine(first, kernel_owner).is_err()
        || ledger.physical_address(first, kernel_owner)
            != Err(frame_ledger::LedgerError::Quarantined)
        || ledger.quarantined_count() != 1
        || ledger.available_count() != 6
    {
        serial_write("ERROR: ownership quarantine invariant failed\n");
        halt_forever();
    }
    serial_write("OWNERSHIP: owner 2 frame = ");
    let service_frame = match ledger.physical_address(second, service_owner) {
        Ok(address) => address,
        Err(_) => {
            serial_write("ERROR: owner 2 cannot access own frame\n");
            halt_forever()
        }
    };
    serial_write_hex(service_frame);
    serial_write("\n");
    serial_write("OWNERSHIP: cross-owner access rejected\n");
    serial_write("OWNERSHIP: quarantined frame blocked from reuse\n");
}

fn serial_write_hex(value: u64) {
    serial_write("0x");
    for shift in (0..16).rev() {
        let nibble = ((value >> (shift * 4)) & 0xf) as u8;
        let digit = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        if !serial_write_byte(digit) {
            halt_forever();
        }
    }
}

/// Called only by the nonreturning, kernel-only assembly exception stubs.
#[no_mangle]
pub extern "C" fn embers_report_exception(vector: u64, rip: u64, error: u64) -> ! {
    match vector {
        6 => serial_write("FAULT: #UD invalid opcode"),
        8 => serial_write("FAULT: #DF double fault"),
        13 => serial_write("FAULT: #GP general protection"),
        14 => serial_write("FAULT: #PF page fault"),
        _ => serial_write("FAULT: unexpected vector"),
    }
    serial_write(" RIP=");
    serial_write_hex(rip);
    serial_write(" CODE=");
    serial_write_hex(error);
    if vector == 14 {
        serial_write(" CR2=");
        serial_write_hex(arch::page_fault_address());
    }
    serial_write("\n");
    halt_forever()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial_write("ERROR: Rust kernel panic\n");
    halt_forever()
}

fn halt_forever() -> ! {
    // SAFETY: Ring 0 and no installed IDT or interrupt handlers yet.
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
    loop {
        // SAFETY: There is deliberately no scheduling in this experiment.
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
