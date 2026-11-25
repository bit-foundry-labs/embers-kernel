#![forbid(unsafe_code)]

use std::env;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use embers_validation::{
    derive_candidates, parse_loads, validate_kernel_entry, validate_kernel_markers,
};

const MAX_ELF_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SERIAL_BYTES: u64 = 4 * 1024 * 1024;

fn read_bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "input exceeds size limit",
        ));
    }
    Ok(data)
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let elf = args
        .next()
        .ok_or("usage: embers-validation ELF [--serial LOG]")?;
    let serial = match args.next() {
        None => None,
        Some(option) if option == "--serial" => Some(args.next().ok_or("missing serial log")?),
        Some(_) => return Err("usage: embers-validation ELF [--serial LOG]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let data = read_bounded(Path::new(&elf), MAX_ELF_BYTES).map_err(|e| e.to_string())?;
    let loads = parse_loads(&data).map_err(|e| format!("ELF load validation: {e:?}"))?;
    validate_kernel_markers(&data).map_err(|e| format!("kernel markers: {e:?}"))?;
    validate_kernel_entry(&data).map_err(|e| format!("kernel symbol: {e:?}"))?;
    if let Some(serial) = serial {
        let bytes =
            read_bounded(Path::new(&serial), MAX_SERIAL_BYTES).map_err(|e| e.to_string())?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let candidates =
            derive_candidates(&loads, text).map_err(|e| format!("Limine metadata: {e:?}"))?;
        for (index, item) in candidates.iter().enumerate() {
            println!(
                "LOAD {index} physical candidate: {:#x}..{:#x}",
                item.start, item.end
            );
        }
    }
    println!("PASS: host ELF validation (no page-table or physical ownership proof)");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}
