//! Early x86-64 exception boundary: single CPU, interrupts disabled.
//! No scheduler, privilege transitions, or owned page tables.
use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut, write_unaligned, write_volatile};

const GDT_CODE: u16 = 0x08;
const GDT_TSS: u16 = 0x18;
const DOUBLE_FAULT_IST: u8 = 1;
const DOUBLE_FAULT_STACK_BYTES: usize = 16 * 1024;

// The entry stack is supplied by Limine; this #DF stack is kernel-owned.
#[repr(align(16))]
struct AlignedStack([u8; DOUBLE_FAULT_STACK_BYTES]);
static mut DOUBLE_FAULT_STACK: AlignedStack = AlignedStack([0; DOUBLE_FAULT_STACK_BYTES]);

// Intel SDM Vol 3: rsp0 begins at byte 4, IST1 at byte 36.
#[repr(C, packed(4))]
#[allow(dead_code)]
struct TaskStateSegment {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

static mut TSS: TaskStateSegment = TaskStateSegment {
    reserved0: 0,
    rsp: [0; 3],
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    io_map_base: size_of::<TaskStateSegment>() as u16,
};
static mut GDT: [u64; 5] = [0; 5];

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct IdtGate {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtGate {
    const EMPTY: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt(handler: usize, ist: u8) -> Self {
        let address = handler as u64;
        Self {
            offset_low: address as u16,
            selector: GDT_CODE,
            ist,
            attributes: 0x8e, // Present, kernel-only, 64-bit interrupt gate.
            offset_middle: (address >> 16) as u16,
            offset_high: (address >> 32) as u32,
            reserved: 0,
        }
    }
}
static mut IDT: [IdtGate; 256] = [IdtGate::EMPTY; 256];

extern "C" {
    fn embers_ud_stub();
    fn embers_df_stub();
    fn embers_gp_stub();
    fn embers_pf_stub();
}

// Interrupt frames enter with [RIP,CS,RFLAGS] for #UD and
// [error_code,RIP,CS,RFLAGS] for #DF/#GP/#PF.
// These are intentionally NONRETURNING stubs. They realign RSP to meet
// the SysV C ABI, do not preserve interrupted registers, and are unsuitable
// for resumable interrupts or ring-3 execution.
global_asm!(
    ".global embers_ud_stub",
    "embers_ud_stub:",
    "mov rdi, 6",
    "mov rsi, [rsp]",
    "xor edx, edx",
    "and rsp, -16",
    "call embers_report_exception",
    "ud2",
    ".global embers_df_stub",
    "embers_df_stub:",
    "mov rdi, 8",
    "mov rsi, [rsp + 8]",
    "mov rdx, [rsp]",
    "and rsp, -16",
    "call embers_report_exception",
    "ud2",
    ".global embers_gp_stub",
    "embers_gp_stub:",
    "mov rdi, 13",
    "mov rsi, [rsp + 8]",
    "mov rdx, [rsp]",
    "and rsp, -16",
    "call embers_report_exception",
    "ud2",
    ".global embers_pf_stub",
    "embers_pf_stub:",
    "mov rdi, 14",
    "mov rsi, [rsp + 8]",
    "mov rdx, [rsp]",
    "and rsp, -16",
    "call embers_report_exception",
    "ud2",
);

fn tss_descriptor(base: u64) -> (u64, u64) {
    let limit = (size_of::<TaskStateSegment>() - 1) as u64;
    let low = (limit & 0xffff)
        | ((base & 0xffff) << 16)
        | (((base >> 16) & 0xff) << 32)
        | (0x89_u64 << 40)
        | (((limit >> 16) & 0xf) << 48)
        | (((base >> 24) & 0xff) << 56);
    (low, base >> 32)
}

/// Establish minimal kernel-owned GDT, TSS, and IDT.
///
/// # Safety
/// Once only, on the BSP at CPL0 with a valid boot stack and mapped writable
/// statics. Interrupts must stay disabled. No concurrent core may use tables.
pub unsafe fn install() {
    // SAFETY: The caller guarantees privileged, exclusive early execution.
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
        let df_top = addr_of_mut!(DOUBLE_FAULT_STACK.0) as u64 + DOUBLE_FAULT_STACK_BYTES as u64;
        write_unaligned(addr_of_mut!(TSS.ist).cast::<u64>(), df_top);

        let (tss_low, tss_high) = tss_descriptor(addr_of!(TSS) as u64);
        let gdt = addr_of_mut!(GDT).cast::<u64>();
        gdt.add(1).write(0x00af_9a00_0000_ffff);
        gdt.add(2).write(0x00cf_9200_0000_ffff);
        gdt.add(3).write(tss_low);
        gdt.add(4).write(tss_high);
        let gdtr = DescriptorTablePointer {
            limit: (size_of::<[u64; 5]>() - 1) as u16,
            base: addr_of!(GDT) as u64,
        };
        asm!("lgdt [{ptr}]", ptr = in(reg) &gdtr, options(readonly, nostack, preserves_flags));

        // Reload CS with a far return; then the remaining segment registers.
        asm!(
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            out("rax") _,
            options(preserves_flags)
        );
        asm!(
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            out("ax") _,
            options(nomem, nostack, preserves_flags)
        );
        asm!("ltr ax", in("ax") GDT_TSS, options(nomem, nostack, preserves_flags));

        let idt = addr_of_mut!(IDT).cast::<IdtGate>();
        write_volatile(idt.add(6), IdtGate::interrupt(embers_ud_stub as usize, 0));
        write_volatile(
            idt.add(8),
            IdtGate::interrupt(embers_df_stub as usize, DOUBLE_FAULT_IST),
        );
        write_volatile(idt.add(13), IdtGate::interrupt(embers_gp_stub as usize, 0));
        write_volatile(idt.add(14), IdtGate::interrupt(embers_pf_stub as usize, 0));
        let idtr = DescriptorTablePointer {
            limit: (size_of::<[IdtGate; 256]>() - 1) as u16,
            base: addr_of!(IDT) as u64,
        };
        asm!("lidt [{ptr}]", ptr = in(reg) &idtr, options(readonly, nostack, preserves_flags));
    }
}

#[cfg(feature = "descriptor-evidence")]
pub struct DescriptorEvidence {
    pub gdt_base: u64,
    pub gdt_bytes: u64,
    pub idt_base: u64,
    pub idt_bytes: u64,
    pub tss_base: u64,
    pub tss_bytes: u64,
    pub ist1_base: u64,
    pub ist1_bytes: u64,
    pub tr_selector: u16,
}

/// Read the installed BSP descriptor state without modifying any register.
/// This validates values, not physical backing, hidden TR cache or CR3 safety.
#[cfg(feature = "descriptor-evidence")]
pub fn sample_descriptor_evidence() -> Option<DescriptorEvidence> {
    let mut gdtr = DescriptorTablePointer { limit: 0, base: 0 };
    let mut idtr = DescriptorTablePointer { limit: 0, base: 0 };
    let tr_selector: u16;
    // SAFETY: Single CPL0 BSP after install(); SGDT, SIDT and STR only write
    // to the provided local output locations, never to privileged registers.
    unsafe {
        asm!("sgdt [{ptr}]", ptr = in(reg) &mut gdtr, options(nostack, preserves_flags));
        asm!("sidt [{ptr}]", ptr = in(reg) &mut idtr, options(nostack, preserves_flags));
        asm!("str {selector:x}", selector = out(reg) tr_selector,
            options(nomem, nostack, preserves_flags));
    }
    // SAFETY: The packed descriptors and TSS.IST1 can be unaligned;
    // read_unaligned uses raw pointers without creating aligned references.
    let (gdt_base, gdt_limit, idt_base, idt_limit, ist1_top) = unsafe {
        (
            core::ptr::read_unaligned(addr_of!(gdtr.base)),
            core::ptr::read_unaligned(addr_of!(gdtr.limit)),
            core::ptr::read_unaligned(addr_of!(idtr.base)),
            core::ptr::read_unaligned(addr_of!(idtr.limit)),
            core::ptr::read_unaligned(addr_of!(TSS.ist).cast::<u64>()),
        )
    };
    let gdt_bytes = size_of::<[u64; 5]>() as u64;
    let idt_bytes = size_of::<[IdtGate; 256]>() as u64;
    let tss_bytes = size_of::<TaskStateSegment>() as u64;
    let ist1_base = addr_of!(DOUBLE_FAULT_STACK.0) as u64;
    let expected_top = ist1_base.checked_add(DOUBLE_FAULT_STACK_BYTES as u64)?;
    if gdt_base != addr_of!(GDT) as u64
        || idt_base != addr_of!(IDT) as u64
        || tr_selector != GDT_TSS
        || gdt_limit as u64 + 1 != gdt_bytes
        || idt_limit as u64 + 1 != idt_bytes
        || ist1_top != expected_top
    {
        return None;
    }
    Some(DescriptorEvidence {
        gdt_base,
        gdt_bytes,
        idt_base,
        idt_bytes,
        tss_base: addr_of!(TSS) as u64,
        tss_bytes,
        ist1_base,
        ist1_bytes: DOUBLE_FAULT_STACK_BYTES as u64,
        tr_selector,
    })
}

/// # Safety
/// CPL0-only deliberate fault. Never use as a normal memory-read API.
#[cfg(any(feature = "fault-pf", feature = "fault-df"))]
pub unsafe fn trigger_page_fault() {
    // SAFETY: QEMU-only deliberate read of an expected-unmapped canonical VA.
    unsafe {
        asm!(
            "mov rax, qword ptr [rax]",
            inout("rax") 0x0000_4444_0000_0000_usize => _,
            options(nostack, readonly, preserves_flags)
        )
    }
}

/// # Safety
/// QEMU-only #DF injection; IDT modifications require exclusive CPL0 access.
#[cfg(feature = "fault-df")]
pub unsafe fn remove_page_fault_gate_for_test() {
    // SAFETY: Single-core QEMU test. The resulting #PF delivery faults.
    unsafe { write_volatile(addr_of_mut!(IDT).cast::<IdtGate>().add(14), IdtGate::EMPTY) };
}

pub fn page_fault_address() -> u64 {
    let address: u64;
    // SAFETY: Only called from the ring-0 exception reporter.
    unsafe {
        asm!("mov {address}, cr2", address = out(reg) address, options(nomem, nostack, preserves_flags));
    }
    address
}
