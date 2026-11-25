use std::process::Command;

#[test]
fn rejects_missing_and_invalid_arguments() {
    let binary = env!("CARGO_BIN_EXE_embers-validation");
    for args in [
        vec![],
        vec!["missing.elf", "--serial"],
        vec!["missing.elf", "--invalid"],
    ] {
        let output = Command::new(binary).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("FAIL:"));
    }
}

#[test]
fn rejects_an_oversized_input_before_parsing() {
    let path = std::env::temp_dir().join(format!("embers-oversized-{}.elf", std::process::id()));
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    drop(file);
    let output = Command::new(env!("CARGO_BIN_EXE_embers-validation"))
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("input exceeds size limit"));
}
