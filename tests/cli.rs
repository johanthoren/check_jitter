use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs

#[test]
fn test_cli_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("--help");

    cmd.assert()
        .code(predicate::eq(3))
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("Options:"));

    Ok(())
}

#[test]
fn test_cli_version() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("--version");

    cmd.assert()
        .code(predicate::eq(3))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));

    Ok(())
}

#[test]
fn test_cli_no_args() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.assert()
        .code(predicate::eq(3))
        .stdout(predicate::str::contains("Usage:"));

    Ok(())
}

#[test]
fn test_cli_with_raw_socket() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("-H")
        .arg("127.0.0.1")
        .arg("-w")
        .arg("100")
        .arg("-c")
        .arg("200");

    if cfg!(target_os = "windows") {
        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with("OK"));
    }

    Ok(())
}

#[test]
fn test_cli_with_dgram_socket() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("-H")
        .arg("127.0.0.1")
        .arg("-w")
        .arg("100")
        .arg("-c")
        .arg("200")
        .arg("-D");

    if cfg!(target_os = "windows") {
        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with("OK"));
    }

    if cfg!(target_os = "macos") {
        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with("OK"));
    }

    Ok(())
}
