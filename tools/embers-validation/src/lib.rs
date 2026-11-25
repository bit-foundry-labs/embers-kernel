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
    SerialMetadata,
    PhysicalRange,
    Marker,
    Symbol,
}

/// Require a defined `kernel_entry` in a bounded ELF64 static symbol table.
pub fn validate_kernel_entry(data: &[u8]) -> Result<(), Error> {
    if data.len() < 64 || !data.starts_with(b"\x7fELF\x02\x01") {
        return Err(Error::Header);
    }
    let shoff = usize::try_from(u64_at(data, 40)?).map_err(|_| Error::Symbol)?;
    let count = usize::from(u16_at(data, 60)?);
    if count == 0 || count > 4096 || u16_at(data, 58)? != 64 {
        return Err(Error::Symbol);
    }
    let end = shoff
        .checked_add(count.checked_mul(64).ok_or(Error::Symbol)?)
        .ok_or(Error::Symbol)?;
    if shoff < 64 || end > data.len() {
        return Err(Error::Symbol);
    }
    for i in 0..count {
        let section = shoff + i * 64;
        if u32_at(data, section + 4)? != 2 {
            continue;
        }
        let link = usize::try_from(u32_at(data, section + 40)?).map_err(|_| Error::Symbol)?;
        if link >= count || u32_at(data, shoff + link * 64 + 4)? != 3 {
            return Err(Error::Symbol);
        }
        let strings_section = shoff + link * 64;
        let slice = |offset: u64, size: u64| -> Result<&[u8], Error> {
            let start = usize::try_from(offset).map_err(|_| Error::Symbol)?;
            let len = usize::try_from(size).map_err(|_| Error::Symbol)?;
            data.get(start..start.checked_add(len).ok_or(Error::Symbol)?)
                .ok_or(Error::Symbol)
        };
        let strings = slice(
            u64_at(data, strings_section + 24)?,
            u64_at(data, strings_section + 32)?,
        )?;
        let symbols = slice(u64_at(data, section + 24)?, u64_at(data, section + 32)?)?;
        if u64_at(data, section + 56)? != 24 || symbols.len() % 24 != 0 {
            return Err(Error::Symbol);
        }
        for symbol in symbols.chunks_exact(24) {
            let name = usize::try_from(u32_at(symbol, 0)?).map_err(|_| Error::Symbol)?;
            let tail = strings.get(name..).ok_or(Error::Symbol)?;
            let length = tail
                .iter()
                .position(|byte| *byte == 0)
                .ok_or(Error::Symbol)?;
            if &tail[..length] == b"kernel_entry" && u16_at(symbol, 6)? != 0 {
                return Ok(());
            }
        }
    }
    Err(Error::Symbol)
}

/// Check the byte markers required by the existing Python kernel ELF check.
/// This does not validate the exported `kernel_entry` symbol.
pub fn validate_kernel_markers(data: &[u8]) -> Result<(), Error> {
    fn contains(data: &[u8], needle: &[u8]) -> bool {
        data.windows(needle.len()).any(|window| window == needle)
    }
    const BASE: &[u8] = b"EMBERS: Rust no_std";
    const LIMINE: [u8; 24] = [
        0xc8, 0xa6, 0x95, 0x5c, 0x2d, 0x2b, 0x56, 0xf9, 0xdc, 0x6b, 0x53, 0x44, 0x49, 0x38, 0x7b,
        0x6a, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    const MODES: [&[u8]; 6] = [
        b"READY: Embers Rust no_std entry reached.",
        b"TEST: intentional invalid opcode",
        b"TEST: intentional page fault",
        b"TEST: intentional double fault through blocked #PF gate",
        b"TEST: supervisor read-only write target = ",
        b"TEST: supervisor NX fetch target = ",
    ];
    if !contains(data, BASE)
        || !contains(data, &LIMINE)
        || MODES.iter().filter(|marker| contains(data, marker)).count() != 1
    {
        return Err(Error::Marker);
    }
    Ok(())
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

/// A numeric candidate derived from Limine metadata, not proof of a mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalCandidate {
    pub start: u64,
    /// The exclusive end may equal 2^64.
    pub end: u128,
}

fn serial_base(serial: &str, kind: &str) -> Result<u64, Error> {
    let prefix = format!("ADDRESS: executable {kind} base = 0x");
    let mut value = None;
    for line in serial.lines().map(str::trim) {
        if let Some(hex) = line.strip_prefix(&prefix) {
            if hex.len() != 16
                || !hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                continue;
            }
            let parsed = u64::from_str_radix(hex, 16).map_err(|_| Error::SerialMetadata)?;
            if value.replace(parsed).is_some() {
                return Err(Error::SerialMetadata);
            }
        }
    }
    value.ok_or(Error::SerialMetadata)
}

/// Derive physical ranges from validated load segments and Limine serial output.
/// These calculations do not establish ownership, lifetime, or page-table mappings.
pub fn derive_candidates(
    loads: &[LoadSegment; 3],
    serial: &str,
) -> Result<[PhysicalCandidate; 3], Error> {
    let physical = serial_base(serial, "physical")?;
    let virtual_base = serial_base(serial, "virtual")?;
    if virtual_base != LINK_BASE || physical == 0 || physical % PAGE_BYTES != 0 {
        return Err(Error::SerialMetadata);
    }
    let mut candidates = [PhysicalCandidate { start: 0, end: 0 }; 3];
    for (candidate, segment) in candidates.iter_mut().zip(loads) {
        let offset = segment
            .vaddr
            .checked_sub(virtual_base)
            .ok_or(Error::PhysicalRange)?;
        let start = physical.checked_add(offset).ok_or(Error::PhysicalRange)?;
        let end = end(start, segment.memsz);
        if end > U64_LIMIT {
            return Err(Error::PhysicalRange);
        }
        *candidate = PhysicalCandidate { start, end };
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

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
    fn kernel_markers_require_base_limine_and_exactly_one_mode() {
        let mut data = b"EMBERS: Rust no_std".to_vec();
        for value in [0xf9562b2d5c95a6c8_u64, 0x6a7b384944536bdc, 6] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        assert_eq!(validate_kernel_markers(&data), Err(Error::Marker));
        data.extend_from_slice(b"READY: Embers Rust no_std entry reached.");
        assert_eq!(validate_kernel_markers(&data), Ok(()));
        data.extend_from_slice(b"TEST: intentional page fault");
        assert_eq!(validate_kernel_markers(&data), Err(Error::Marker));
        assert_eq!(validate_kernel_markers(b""), Err(Error::Marker));
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

    #[test]
    fn limine_candidates_reject_invalid_metadata_and_overflow() {
        let loads = parse_loads(&fixture()).unwrap();
        let serial = "ADDRESS: executable physical base = 0x000000000ff44000\n\
                      ADDRESS: executable virtual base = 0xffffffff80000000\n";
        let candidates = derive_candidates(&loads, serial).unwrap();
        assert_eq!(
            candidates[0],
            PhysicalCandidate {
                start: 0x0ff44000,
                end: 0x0ff44100
            }
        );
        assert_eq!(candidates[2].start, 0x0ff46000);
        assert!(derive_candidates(&loads, &(serial.to_owned() + serial)).is_err());
        assert!(derive_candidates(&loads, &serial.replace("0ff44000", "0ff44001")).is_err());
        assert!(derive_candidates(
            &loads,
            &serial.replace("ffffffff80000000", "ffff800000000000")
        )
        .is_err());
        assert!(derive_candidates(
            &loads,
            &serial.replace("000000000ff44000", "fffffffffffff000")
        )
        .is_err());
    }

    #[test]
    fn load_decisions_match_python_reference_on_selected_mutations() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let oracle = r#"
import sys
sys.path.insert(0, 'scripts')
from check_elf_loads import parse_loads
try:
    parse_loads(sys.stdin.buffer.read())
except ValueError:
    sys.exit(1)
"#;
        let good = fixture();
        let mut cases = vec![good.clone(), good[..90].to_vec()];
        for (offset, bytes) in [
            (68, 7_u32.to_le_bytes().to_vec()),
            (64 + 56 + 4, 6_u32.to_le_bytes().to_vec()),
            (64 + 56 + 16, LINK_BASE.to_le_bytes().to_vec()),
            (64 + 48, 4095_u64.to_le_bytes().to_vec()),
            (32, u64::MAX.to_le_bytes().to_vec()),
        ] {
            let mut data = good.clone();
            put(&mut data, offset, &bytes);
            cases.push(data);
        }
        for data in cases {
            let mut child = Command::new("python3")
                .args(["-c", oracle])
                .current_dir(&root)
                .stdin(Stdio::piped())
                .spawn()
                .expect("python3 is required for differential tests");
            child.stdin.take().unwrap().write_all(&data).unwrap();
            let python_accepts = child.wait().unwrap().success();
            assert_eq!(parse_loads(&data).is_ok(), python_accepts);
        }
    }

    #[test]
    fn limine_decisions_and_ranges_match_python_reference() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let oracle = r#"
import sys
sys.path.insert(0, 'scripts')
from check_elf_loads import derive_candidates, parse_loads
try:
    candidates = derive_candidates(parse_loads(sys.stdin.buffer.read()), sys.argv[1])
except ValueError:
    sys.exit(1)
for start, end in candidates:
    print(f'{start:x} {end:x}')
"#;
        let data = fixture();
        let loads = parse_loads(&data).unwrap();
        let serial = "ADDRESS: executable physical base = 0x000000000ff44000\n\
                      ADDRESS: executable virtual base = 0xffffffff80000000\n";
        let cases = [
            serial.to_owned(),
            serial.to_owned() + serial,
            serial.replace("0ff44000", "0ff44001"),
            serial.replace("ffffffff80000000", "ffff800000000000"),
            serial.replace("000000000ff44000", "fffffffffffff000"),
            serial.replace("physical", "missing"),
        ];
        for input in cases {
            let mut child = Command::new("python3")
                .args(["-c", oracle, &input])
                .current_dir(&root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("python3 is required for differential tests");
            child.stdin.take().unwrap().write_all(&data).unwrap();
            let output = child.wait_with_output().unwrap();
            let rust = derive_candidates(&loads, &input);
            assert_eq!(rust.is_ok(), output.status.success(), "serial: {input:?}");
            if let Ok(candidates) = rust {
                let ranges: String = candidates
                    .iter()
                    .map(|item| format!("{:x} {:x}\n", item.start, item.end))
                    .collect();
                assert_eq!(ranges.as_bytes(), output.stdout);
            }
        }
    }

    #[test]
    fn real_ci_serial_addresses_match_python_when_provided() {
        let Some(path) = std::env::var_os("EMBERS_TEST_SERIAL") else {
            eprintln!("EMBERS_TEST_SERIAL is not set; real-serial comparison not run");
            return;
        };
        let serial = std::fs::read_to_string(path).expect("read EMBERS_TEST_SERIAL");
        let data = fixture();
        let loads = parse_loads(&data).unwrap();
        let actual = derive_candidates(&loads, &serial).expect("Rust rejected serial addresses");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let oracle = "import sys; sys.path.insert(0,'scripts'); from check_elf_loads import parse_loads,derive_candidates; print(''.join(f'{a:x} {b:x}\\n' for a,b in derive_candidates(parse_loads(sys.stdin.buffer.read()),sys.argv[1])),end='')";
        let mut child = Command::new("python3")
            .args(["-c", oracle, &serial])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&data).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let ranges: String = actual
            .iter()
            .map(|x| format!("{:x} {:x}\n", x.start, x.end))
            .collect();
        assert_eq!(ranges.as_bytes(), output.stdout);
    }

    #[test]
    fn real_kernel_load_layout_matches_python_when_provided() {
        let Some(path) = std::env::var_os("EMBERS_TEST_ELF") else {
            eprintln!("EMBERS_TEST_ELF is not set; real-artifact comparison not run");
            return;
        };
        let data = std::fs::read(&path).expect("read EMBERS_TEST_ELF");
        validate_kernel_markers(&data).expect("Rust rejected kernel markers");
        validate_kernel_entry(&data).expect("Rust rejected kernel entry symbol");
        assert!(validate_kernel_entry(&data[..63]).is_err());
        let mut malformed = data.clone();
        put(&mut malformed, 40, &u64::MAX.to_le_bytes());
        assert!(validate_kernel_entry(&malformed).is_err());
        let mut malformed = data.clone();
        put(&mut malformed, 60, &0_u16.to_le_bytes());
        assert!(validate_kernel_entry(&malformed).is_err());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("python3")
            .arg(root.join("scripts/check_elf_loads.py"))
            .arg(path)
            .output()
            .expect("python3 is required for differential tests");
        assert!(output.status.success(), "Python rejected the kernel ELF");
        let loads = parse_loads(&data).expect("Rust rejected the kernel ELF");
        let python = String::from_utf8(output.stdout).unwrap();
        for (i, seg) in loads.iter().enumerate() {
            let expected = format!(
                "ELF LOAD {i}: flags={} VA=0x{:016x} mem_bytes=0x{:x} file_bytes=0x{:x}",
                seg.flags, seg.vaddr, seg.memsz, seg.filesz
            );
            assert!(python.lines().any(|line| line == expected));
        }
    }

    #[test]
    fn real_kernel_marker_and_symbol_mutations_match_python_when_provided() {
        let Some(path) = std::env::var_os("EMBERS_TEST_ELF") else {
            eprintln!("EMBERS_TEST_ELF is not set; marker differential test not run");
            return;
        };
        let original = std::fs::read(path).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let temporary =
            std::env::temp_dir().join(format!("embers-elf-differential-{}", std::process::id()));
        for needle in [
            b"EMBERS: Rust no_std".as_slice(),
            b"kernel_entry".as_slice(),
        ] {
            let mut data = original.clone();
            let mut changed = 0;
            for position in 0..data.len().saturating_sub(needle.len()) + 1 {
                if data.get(position..position + needle.len()) == Some(needle) {
                    data[position] ^= 1;
                    changed += 1;
                }
            }
            assert!(changed > 0);
            std::fs::write(&temporary, &data).unwrap();
            let python = Command::new("python3")
                .arg(root.join("scripts/check_rust_elf.py"))
                .arg(&temporary)
                .output()
                .unwrap();
            let rust_accepts =
                validate_kernel_markers(&data).is_ok() && validate_kernel_entry(&data).is_ok();
            assert!(!python.status.success());
            assert_eq!(rust_accepts, python.status.success());
        }
        std::fs::remove_file(temporary).unwrap();
    }
}
