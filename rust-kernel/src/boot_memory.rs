//! Bounded translation of Limine's memory map into pure-Rust frame bookkeeping.
//! This trusts the bootloader's pointers until Embers owns page tables.
use crate::memory::{
    claim_boot_pool_permit, select_boot_pool, BootMapSnapshot, FramePool, MapError, Region,
    MAX_BOOT_REGIONS, REQUIRED_BOOT_PAGES,
};
use core::ptr::{addr_of, read_volatile};

#[repr(C)]
struct LimineMemoryMapRequest {
    id: [u64; 4],
    revision: u64,
    response: *const LimineMemoryMapResponse,
}

#[repr(C)]
struct LimineMemoryMapResponse {
    revision: u64,
    entry_count: u64,
    entries: *const *const LimineMemoryMapEntry,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct LimineMemoryMapEntry {
    base: u64,
    length: u64,
    kind: u64,
}

#[used]
#[link_section = ".limine_requests"]
static mut MEMORY_MAP_REQUEST: LimineMemoryMapRequest = LimineMemoryMapRequest {
    id: [
        0xc7b1dd30df4c8b88,
        0x0a82e883a194f07b,
        0x67cf3d9d378a806f,
        0xe304acdfc50c3c62,
    ],
    revision: 0,
    response: core::ptr::null(),
};

#[derive(Debug)]
pub enum BootMapError {
    AlreadyIssued,
    NoResponse,
    InvalidCount,
    MissingEntry,
    InvalidMap(MapError),
}

/// A selected numeric frame pool and the copied map that justified it.
/// Neither member is hardware-enforced resource authority.
pub struct BootPoolSelection {
    pub pool: FramePool,
    pub map: BootMapSnapshot,
}

/// Select a usable physical span and preserve the trusted handoff's map
/// without retaining Limine-owned pointers. Does not write frames, change
/// CR3, validate DMA, or acquire hardware-enforced ownership.
pub fn initial_frame_pool() -> Result<BootPoolSelection, BootMapError> {
    let permit = claim_boot_pool_permit().ok_or(BootMapError::AlreadyIssued)?;
    // SAFETY: During the BSP-only handoff, Limine is the sole writer of
    // response before entry and never mutates it after transfer of control.
    let response = unsafe { read_volatile(addr_of!(MEMORY_MAP_REQUEST.response)) };
    if response.is_null() {
        return Err(BootMapError::NoResponse);
    }

    // SAFETY: Until we own paging, we trust Limine to supply an aligned,
    // mapped, initialized and stable response structure.
    let header = unsafe { read_volatile(response) };
    let count = usize::try_from(header.entry_count).map_err(|_| BootMapError::InvalidCount)?;
    if count == 0 || count > MAX_BOOT_REGIONS || header.entries.is_null() {
        return Err(BootMapError::InvalidCount);
    }

    // Fixed maximum prevents unbounded parsing and boot stack growth.
    let mut regions = [Region::EMPTY; MAX_BOOT_REGIONS];
    for (index, slot) in regions[..count].iter_mut().enumerate() {
        // SAFETY: The pointer array is a Limine-owned allocation of at least
        // count valid entries, mapped and stable under the above assumption.
        let entry = unsafe { read_volatile(header.entries.add(index)) };
        if entry.is_null() {
            return Err(BootMapError::MissingEntry);
        }
        // SAFETY: The same Limine contract supplies a readable, aligned
        // entry consisting only of three initialized u64 fields.
        let raw = unsafe { read_volatile(entry) };
        *slot = Region {
            base: raw.base,
            length: raw.length,
            kind: raw.kind,
        };
    }

    // Validate and select using the same copied entries before retaining
    // them. We never keep references into bootloader-owned response memory.
    let pool = select_boot_pool(permit, &regions[..count], REQUIRED_BOOT_PAGES)
        .map_err(BootMapError::InvalidMap)?;
    let map = BootMapSnapshot::from_copied_entries(regions, count)
        .ok_or(BootMapError::InvalidCount)?;
    Ok(BootPoolSelection { pool, map })
}
