use std::process::Command;

#[test]
fn help_lists_package_sources() {
    let output = Command::new(env!("CARGO_BIN_EXE_kitenest"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("search"));
    assert!(help.contains("upgrade"));
}

#[test]
fn unsupported_source_fails_with_a_readable_message() {
    let output = Command::new(env!("CARGO_BIN_EXE_kitenest"))
        .args(["search", "firefox", "--source", "pacman"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("possible values: rpm, flatpak, appimage, all"));
}
