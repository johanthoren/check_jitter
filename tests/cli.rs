use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs

#[test]
fn test_cli_help() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("Options:"));

    Ok(())
}

#[test]
fn test_cli_version() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));

    Ok(())
}

#[test]
fn test_cli_no_args() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("check_jitter")?;

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));

    Ok(())
}
