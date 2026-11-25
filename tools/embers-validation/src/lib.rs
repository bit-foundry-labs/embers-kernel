#![forbid(unsafe_code)]

//! Host-side ELF validation. No page-table or physical ownership proof.

const LINK_BASE: u64 = 0xffff_ffff_8000_0000;
const PAGE_BYTES: u64 = 4096;
const U64_LIMIT: u128 = 1_u128 << 64;
const FLAGS: [u32; 3] = [5, 4, 6];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LoadSegment {
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Header,
    Table,
    Length,
    FileExtent,
    Alignment,
    AddressRange,
    WritableExecutable,
    Count,
    Flags,
    Overlap,
    Entry,
}

fn read<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N], Error> {
    let end = offset.checked_add(N).ok_or(Error::Table)?;
    data.get(offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Table)
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(read(data, offset)?))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(read(data, offset)?))
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(read(data, offset)?))
}

fn end(start: u64, size: u64) -> u128 {
    u128::from(start) + u128::from(size)
}

/// Validate the current Embers linker contract, not arbitrary ELF executables.
pub fn parse_loads(data: &[u8]) -> Result<[LoadSegment; 3], Error> {
    if data.len() < 64
        || !data.starts_with(b"\x7fELF\x02\x01")
        || (u16_at(data, 16)?, u16_at(data, 18)?, u32_at(data, 20)?) != (2, 62, 1)
    {
        return Err(Error::Header);
    }
    let entry = u64_at(data, 24)?;
    let phoff = usize::try_from(u64_at(data, 32)?).map_err(|_| Error::Table)?;
    let phnum = usize::from(u16_at(data, 56)?);
    if u16_at(data, 52)? != 64
        || u16_at(data, 54)? != 56
        || !(1..=64).contains(&phnum)
        || phoff < 64
        || phoff
            .checked_add(phnum * 56)
            .is_none_or(|table_end| table_end > data.len())
    {
        return Err(Error::Table);
    }
    let mut loads = [LoadSegment::default(); 3];
    let mut count = 0;
    for i in 0..phnum {
        let p = phoff + i * 56;
        if u32_at(data, p)? != 1 {
            continue;
        }
        let segment = LoadSegment {
            flags: u32_at(data, p + 4)?,
            offset: u64_at(data, p + 8)?,
            vaddr: u64_at(data, p + 16)?,
            filesz: u64_at(data, p + 32)?,
            memsz: u64_at(data, p + 40)?,
            align: u64_at(data, p + 48)?,
        };
        if segment.memsz == 0 || segment.filesz > segment.memsz {
            return Err(Error::Length);
        }
        if end(segment.offset, segment.filesz) > data.len() as u128 {
            return Err(Error::FileExtent);
        }
        if segment.align < PAGE_BYTES
            || !segment.align.is_power_of_two()
            || segment.vaddr % segment.align != segment.offset % segment.align
        {
            return Err(Error::Alignment);
        }
        // This higher-half range is entirely canonical under 48-bit paging.
        // Represent the exclusive end in u128, including exactly 2^64.
        if segment.vaddr < LINK_BASE || end(segment.vaddr, segment.memsz) > U64_LIMIT {
            return Err(Error::AddressRange);
        }
        if segment.flags & 3 == 3 {
            return Err(Error::WritableExecutable);
        }
        let slot = loads.get_mut(count).ok_or(Error::Count)?;
        *slot = segment;
        count += 1;
    }
    if count != FLAGS.len() {
        return Err(Error::Count);
    }
    if loads.map(|segment| segment.flags) != FLAGS {
        return Err(Error::Flags);
    }
    for pair in loads.windows(2) {
        if end(pair[0].vaddr, pair[0].memsz) > u128::from(pair[1].vaddr) {
            return Err(Error::Overlap);
        }
    }
    if entry < loads[0].vaddr || u128::from(entry) >= end(loads[0].vaddr, loads[0].memsz) {
        return Err(Error::Entry);
    }
    Ok(loads)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(data: &mut [u8], offset: usize, bytes: &[u8]) {
        data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    fn fixture() -> Vec<u8> {
        let mut data = vec![0; 0x4000];
        put(&mut data, 0, b"\x7fELF\x02\x01");
        put(&mut data, 16, &2_u16.to_le_bytes());
        put(&mut data, 18, &62_u16.to_le_bytes());
        put(&mut data, 20, &1_u32.to_le_bytes());
        put(&mut data, 24, &(LINK_BASE + 0x10).to_le_bytes());
        put(&mut data, 32, &64_u64.to_le_bytes());
        put(&mut data, 52, &64_u16.to_le_bytes());
        put(&mut data, 54, &56_u16.to_le_bytes());
        put(&mut data, 56, &3_u16.to_le_bytes());
        for (i, flags) in FLAGS.into_iter().enumerate() {
            let p = 64 + i * 56;
            put(&mut data, p, &1_u32.to_le_bytes());
            put(&mut data, p + 4, &flags.to_le_bytes());
            put(
                &mut data,
                p + 8,
                &((i as u64 + 1) * PAGE_BYTES).to_le_bytes(),
            );
            put(
                &mut data,
                p + 16,
                &(LINK_BASE + i as u64 * PAGE_BYTES).to_le_bytes(),
            );
            put(&mut data, p + 32, &32_u64.to_le_bytes());
            put(&mut data, p + 40, &0x100_u64.to_le_bytes());
            put(&mut data, p + 48, &PAGE_BYTES.to_le_bytes());
        }
        data
    }

    #[test]
    fn accepts_linker_layout() {
        let loads = parse_loads(&fixture()).unwrap();
        assert_eq!(loads.map(|segment| segment.flags), FLAGS);
        assert_eq!(loads[2].vaddr, LINK_BASE + 2 * PAGE_BYTES);
        assert_eq!(loads[0].filesz, 32);
    }

    #[test]
    fn rejects_every_truncated_header_or_table() {
        let data = fixture();
        for len in 0..64 + 3 * 56 {
            assert!(parse_loads(&data[..len]).is_err(), "length {len}");
        }
    }

    #[test]
    fn rejects_bad_flags_and_overlap() {
        let mut data = fixture();
        put(&mut data, 68, &7_u32.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::WritableExecutable));
        let mut data = fixture();
        put(&mut data, 64 + 56 + 4, &6_u32.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Flags));
        let mut data = fixture();
        put(&mut data, 64 + 56 + 16, &LINK_BASE.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Overlap));
    }

    #[test]
    fn rejects_invalid_extents_and_alignment() {
        let mut data = fixture();
        put(&mut data, 64 + 32, &0x101_u64.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Length));
        let mut data = fixture();
        put(&mut data, 64 + 8, &u64::MAX.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::FileExtent));
        let mut data = fixture();
        put(&mut data, 64 + 48, &4095_u64.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Alignment));
    }

    #[test]
    fn handles_exclusive_end_without_wrap() {
        let mut data = fixture();
        let p = 64 + 2 * 56;
        put(&mut data, p + 16, &(u64::MAX - 4095).to_le_bytes());
        put(&mut data, p + 40, &4096_u64.to_le_bytes());
        assert!(parse_loads(&data).is_ok());
        put(&mut data, p + 40, &4097_u64.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::AddressRange));
    }

    #[test]
    fn rejects_bad_entry_and_table_offset() {
        let mut data = fixture();
        put(&mut data, 24, &(LINK_BASE + PAGE_BYTES).to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Entry));
        let mut data = fixture();
        put(&mut data, 32, &u64::MAX.to_le_bytes());
        assert_eq!(parse_loads(&data), Err(Error::Table));
    }
}
