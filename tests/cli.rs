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

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    #[ignore] // This test is a bit flaky depending on the system configuration.
    #[test]
    fn test_cli_with_raw_socket() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-w")
            .arg("100")
            .arg("-c")
            .arg("200");

        cmd.assert()
            .code(predicate::eq(3))
            .stdout(predicate::str::contains("insufficient permissions"));

        Ok(())
    }

    #[ignore] // This test is a bit flaky depending on the system configuration.
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

        cmd.assert()
            .code(predicate::eq(3))
            .stdout(predicate::str::contains("DecodeV4Error"));

        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    #[test]
    fn test_cli_with_raw_socket() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-w")
            .arg("100")
            .arg("-c")
            .arg("200");

        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with("OK"));

        Ok(())
    }

    #[test]
    fn test_cli_with_raw_socket_verbose_median() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-w")
            .arg("300")
            .arg("-c")
            .arg("500")
            .arg("-a")
            .arg("median")
            .arg("-p")
            .arg("5")
            .arg("-s")
            .arg("3")
            .arg("-t")
            .arg("100")
            .arg("-vvv");

        cmd.assert()
            .success()
            .stderr(predicate::str::is_match("Aggregation method: +Median").unwrap())
            .stderr(predicate::str::is_match("Socket type: +Raw").unwrap())
            .stderr(predicate::str::is_match("Decimal precision: +5").unwrap())
            .stderr(predicate::str::is_match("Sample size: +3").unwrap())
            .stderr(predicate::str::is_match("Timeout per ping: +100ms").unwrap())
            .stdout(predicate::str::starts_with("OK - Median Jitter:"));

        Ok(())
    }

    #[test]
    fn test_cli_with_dgram_socket() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;
        let w_err = "The requested protocol has not been configured into the system, or no implementation for it exists.";

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-w")
            .arg("100")
            .arg("-c")
            .arg("200")
            .arg("-D");

        cmd.assert()
            .code(predicate::eq(3))
            .stdout(predicate::str::contains(w_err));

        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    #[test]
    fn test_cli_with_raw_socket() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-w")
            .arg("100")
            .arg("-c")
            .arg("200");

        cmd.assert()
            .code(predicate::eq(3))
            .stdout(predicate::str::contains("insufficient permissions"));

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

        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with("OK - Average Jitter:"));

        Ok(())
    }

    #[test]
    fn test_cli_with_dgram_socket_verbose_median() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("check_jitter")?;

        cmd.arg("-H")
            .arg("127.0.0.1")
            .arg("-D")
            .arg("-w")
            .arg("300")
            .arg("-c")
            .arg("500")
            .arg("-a")
            .arg("median")
            .arg("-p")
            .arg("5")
            .arg("-s")
            .arg("3")
            .arg("-t")
            .arg("100")
            .arg("-vvv");

        cmd.assert()
            .success()
            .stderr(predicate::str::is_match("Aggregation method: +Median").unwrap())
            .stderr(predicate::str::is_match("Socket type: +Datagram").unwrap())
            .stderr(predicate::str::is_match("Decimal precision: +5").unwrap())
            .stderr(predicate::str::is_match("Sample size: +3").unwrap())
            .stderr(predicate::str::is_match("Timeout per ping: +100ms").unwrap())
            .stdout(predicate::str::starts_with("OK - Median Jitter:"));

        Ok(())
    }
}
