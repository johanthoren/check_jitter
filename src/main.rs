use check_jitter::*;
use clap::Parser;
use nagios_range::NagiosRange;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process;
use std::time::Duration;

const ABOUT_TEXT: &str = r#"
A Nagios compatible plugin that measures network jitter.

Thresholds are defined using Nagios range syntax. Examples:
+------------------+-------------------------------------------------+
| Range definition | Generate an alert if x...                       |
+------------------+-------------------------------------------------+
| 10               | < 0 or > 10, (outside the range of {0 .. 10})   |
+------------------+-------------------------------------------------+
| 10:              | < 10, (outside {10 .. ∞})                       |
+------------------+-------------------------------------------------+
| ~:10             | > 10, (outside the range of {-∞ .. 10})         |
+------------------+-------------------------------------------------+
| 10:20            | < 10 or > 20, (outside the range of {10 .. 20}) |
+------------------+-------------------------------------------------+
| @10:20           | ≥ 10 and ≤ 20, (inside the range of {10 .. 20}) |
+------------------+-------------------------------------------------+
"#;

#[derive(Parser, Debug)]
#[command(author, version, long_about = None, about = ABOUT_TEXT)]
struct Args {
    /// Critical limit for network jitter in milliseconds
    #[arg(short, long)]
    critical: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Hostname or IP address to ping
    #[arg(long, short = 'H')]
    host: String,

    /// Minimum interval between ping samples in milliseconds
    #[arg(short, long, default_value = "0")]
    min_interval: u64,

    /// Maximum interval between ping samples in milliseconds
    #[arg(short, long, default_value = "0", short = 'M')]
    max_interval: u64,

    /// Precision of the output decimal places
    #[arg(short, long, default_value = "3")]
    precision: u8,

    /// Number of pings to send
    #[arg(short, long, default_value = "10")]
    samples: u16,

    /// Timeout in milliseconds per individual ping check
    #[arg(short, long, default_value = "1000")]
    timeout: u64,

    /// Warning limit for network jitter in milliseconds
    #[arg(short, long)]
    warning: Option<String>,
}

fn exit_with_message(status: Status) {
    println!("{}", status);
    process::exit(status.to_int());
}

fn validate_host(s: &str) -> Result<String, CheckJitterError> {
    if s.parse::<Ipv4Addr>().is_ok() {
        return Ok(s.to_string());
    }
    if s.parse::<Ipv6Addr>().is_ok() {
        return Ok(s.to_string());
    }
    match url::Host::parse(s) {
        Ok(url::Host::Domain(_)) | Ok(url::Host::Ipv4(_)) | Ok(url::Host::Ipv6(_)) => {
            Ok(s.to_string())
        }
        _ => Err(CheckJitterError::InvalidIP(s.to_string())),
    }
}

/// Check network jitter.
fn main() {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(if args.debug { 4 } else { 0 })
        .init()
        // TODO: Don't use unwrap().
        .unwrap();

    if args.min_interval > args.max_interval {
        exit_with_message(Status::Unknown(UnkownVariant::InvalidMinMaxInterval(
            args.min_interval,
            args.max_interval,
        )))
    }

    if validate_host(&args.host).is_err() {
        exit_with_message(Status::Unknown(UnkownVariant::InvalidHost(
            args.host.clone(),
        )))
    }

    if args.warning.is_none() && args.critical.is_none() {
        exit_with_message(Status::Unknown(UnkownVariant::NoThresholds))
    }

    let mut warning: Option<NagiosRange> = None;

    if let Some(w) = args.warning {
        let w_range = NagiosRange::from(&w);
        match w_range {
            Ok(r) => warning = Some(r),
            Err(e) => exit_with_message(Status::Unknown(UnkownVariant::RangeParseError(w, e))),
        }
    }

    let mut critical: Option<NagiosRange> = None;

    if let Some(c) = args.critical {
        let c_range = NagiosRange::from(&c);
        match c_range {
            Ok(r) => critical = Some(r),
            Err(e) => exit_with_message(Status::Unknown(UnkownVariant::RangeParseError(c, e))),
        }
    }

    let thresholds = Thresholds { warning, critical };
    let timeout = Duration::from_millis(args.timeout);

    match get_jitter(
        &args.host,
        args.samples as usize,
        timeout,
        args.precision,
        args.min_interval,
        args.max_interval,
    ) {
        Ok(jitter) => exit_with_message(evaluate_thresholds(jitter, &thresholds)),
        Err(e) => exit_with_message(Status::Unknown(UnkownVariant::Error(e))),
    };
}
