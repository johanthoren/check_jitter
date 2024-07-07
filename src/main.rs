use check_jitter::*;
use chrono::Utc;
use clap::{value_parser, Parser};
use log::debug;
use nagios_range::NagiosRange;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process;
use std::time::Duration;

const ABOUT_TEXT: &str = r#"
check_jitter - A Nagios compatible plugin that measures network jitter.

SAMPLE INTERVALS

When -m and -M are both set to 0, the plugin will send pings immediately after
receiving a response.

When -m and -M are set to the same value, the plugin will send pings at a fixed
interval.

When -m and -M are set to different values, the plugin will send pings at random
intervals between the two values.

-m must be less than or equal to -M.

THRESHOLD SYNTAX

Thresholds are defined using Nagios range syntax.

Example ranges:
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
+------------------+-------------------------------------------------+"#;

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
    #[arg(short, long, default_value = "10", value_parser = value_parser!(u8).range(3..))]
    samples: u8,

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

fn setup_logger(debug: bool) -> Result<(), fern::InitError> {
    if debug {
        fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "{} [{}] [{}] [{}:{}] {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.target(),
                    record.level(),
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    message
                ))
            })
            .level(log::LevelFilter::Debug)
            .chain(std::io::stderr())
            .apply()?;
    } else {
        fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "{} [{}] [{}] {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.target(),
                    record.level(),
                    message
                ))
            })
            .level(log::LevelFilter::Info)
            .chain(std::io::stderr())
            .apply()?;
    }
    Ok(())
}

/// Check network jitter.
fn main() {
    let args = Args::parse();

    if let Err(e) = setup_logger(args.debug) {
        exit_with_message(Status::Unknown(UnkownVariant::FailedToInitLogger(
            e.to_string(),
        )))
    }

    if args.min_interval > args.max_interval {
        exit_with_message(Status::Unknown(UnkownVariant::InvalidMinMaxInterval(
            args.min_interval,
            args.max_interval,
        )))
    }

    if validate_host(&args.host).is_err() {
        exit_with_message(Status::Unknown(UnkownVariant::InvalidAddr(
            args.host.clone(),
        )))
    }

    if args.warning.is_none() && args.critical.is_none() {
        exit_with_message(Status::Unknown(UnkownVariant::NoThresholds))
    }

    let warning: Option<NagiosRange> = match args.warning {
        Some(w) => NagiosRange::from(w.as_str())
            .map_err(|e| exit_with_message(Status::Unknown(UnkownVariant::RangeParseError(w, e))))
            .ok(),
        None => None,
    };

    let critical: Option<NagiosRange> = match args.critical {
        Some(c) => NagiosRange::from(c.as_str())
            .map_err(|e| exit_with_message(Status::Unknown(UnkownVariant::RangeParseError(c, e))))
            .ok(),
        None => None,
    };

    let thresholds = Thresholds::new(warning, critical);
    let timeout = Duration::from_millis(args.timeout);

    debug!("{:<34}{}", "Will check jitter for host:", args.host);
    debug!("{:<34}{}", "Samples to send:", args.samples);
    debug!("{:<34}{}ms", "Timeout per ping:", args.timeout);
    debug!(
        "{:<34}{}ms",
        "Minimum wait time between pings:", args.min_interval
    );
    debug!(
        "{:<34}{}ms",
        "Maximum wait time between pings:", args.max_interval
    );
    debug!("{:<34}{}", "Decimal precision:", args.precision);
    debug!("{:<34}{:?}", "Warning threshold:", warning);
    debug!("{:<34}{:?}", "Critical threshold:", critical);

    match get_jitter(
        &args.host,
        args.samples,
        timeout,
        args.precision,
        args.min_interval,
        args.max_interval,
    ) {
        Ok(jitter) => exit_with_message(evaluate_thresholds(jitter, &thresholds)),
        Err(e) => exit_with_message(Status::Unknown(UnkownVariant::Error(e))),
    };
}
