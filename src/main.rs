use check_jitter::*;
use chrono::Utc;
use clap::{value_parser, ArgAction::Count, Parser};
use log::{info, LevelFilter};
use nagios_range::NagiosRange as ThresholdRange;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::process;
use std::time::Duration;

const ABOUT_TEXT: &str = r#"
check_jitter - A monitoring plugin that measures network jitter.

AGGREGATION METHOD

The plugin can aggregate the deltas from multiple samples in the following ways:
- average: the average of all deltas (arithmetic mean) [default]
- median: the median of all deltas
- max: the maximum of all deltas
- min: the minimum of all deltas

HOSTNAME

If the hostname resolves to multiple IP addresses, the plugin will use the first
address returned by the DNS resolver and skip the rest.

While using a hostname is supported, consider using IP addresses instead. It's
better to set up multiple tests to cover each IP individually rather than relying
on hostname resolution.

SAMPLES

The number of pings to send to the target host. Must be greater than 2.

SAMPLE INTERVALS

When -m and -M are both set to 0, the plugin will send pings immediately after
receiving a response.

When -m and -M are set to the same value, the plugin will send pings at a fixed
interval.

When -m and -M are set to different values, the plugin will send pings at random
intervals between the two values.

-m must be less than or equal to -M.

THRESHOLD SYNTAX

Thresholds are defined using monitoring plugin range syntax.

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
    /// Aggregation method to use for multiple samples
    #[arg(short, long, default_value = "average")]
    aggregation_method: AggregationMethod,

    /// Critical limit for network jitter in milliseconds
    #[arg(short, long)]
    critical: Option<String>,

    /// Use a datagram socket instead of a raw socket (expert option)
    #[arg(long, short = 'D')]
    dgram_socket: bool,

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

    /// Sample size: the number of pings to send
    #[arg(short, long, default_value = "10", value_parser = value_parser!(u8).range(3..))]
    samples: u8,

    /// Timeout in milliseconds per individual ping check
    #[arg(short, long, default_value = "1000")]
    timeout: u64,

    /// Warning limit for network jitter in milliseconds
    #[arg(short, long)]
    warning: Option<String>,

    /// Enable verbose output. Use multiple times to increase verbosity (e.g. -vvv)
    #[arg(short, long, action = Count, value_parser = value_parser!(u8).range(0..=3))]
    verbose: u8,
}

fn exit_with_message(status: Status) -> ! {
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

fn select_and_init_logger(verbosity: u8) -> Result<(), fern::InitError> {
    setup_logger(match verbosity {
        3 => (LevelFilter::Debug, true),
        2 => (LevelFilter::Info, false),
        _ => (LevelFilter::Error, false),
    })
}

fn setup_logger((level, include_file_info): (LevelFilter, bool)) -> Result<(), fern::InitError> {
    let dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            let base_format = format!(
                "{} [{}] [{}]",
                Utc::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.target(),
                record.level()
            );

            let full_format = if include_file_info {
                format!(
                    "{} [{}:{}] {}",
                    base_format,
                    record.file().unwrap_or("unknown"),
                    record.line().unwrap_or(0),
                    message
                )
            } else {
                format!("{} {}", base_format, message)
            };

            out.finish(format_args!("{}", full_format))
        })
        .level(level)
        .chain(std::io::stderr());

    dispatch.apply()?;
    Ok(())
}

/// Check network jitter.
fn main() {
    // According to monitoring-plugins guidelines, exit code 3 is used for "UNKNOWN" and
    // should be used for the --help and --version flags.
    let args = Args::try_parse().unwrap_or_else(|e| match e.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            print!("{}", e);
            std::process::exit(3);
        }
        _ => exit_with_message(Status::Unknown(UnknownVariant::ClapError(e.to_string()))),
    });

    if let Err(e) = select_and_init_logger(args.verbose) {
        exit_with_message(Status::Unknown(UnknownVariant::FailedToInitLogger(
            e.to_string(),
        )))
    }

    if args.min_interval > args.max_interval {
        exit_with_message(Status::Unknown(UnknownVariant::InvalidMinMaxInterval(
            args.min_interval,
            args.max_interval,
        )))
    }

    if validate_host(&args.host).is_err() {
        exit_with_message(Status::Unknown(UnknownVariant::InvalidAddr(
            args.host.clone(),
        )))
    }

    if args.warning.is_none() && args.critical.is_none() {
        exit_with_message(Status::Unknown(UnknownVariant::NoThresholds))
    }

    let warning: Option<ThresholdRange> = match args.warning {
        Some(w) => ThresholdRange::from(w.as_str())
            .map_err(|e| exit_with_message(Status::Unknown(UnknownVariant::RangeParseError(w, e))))
            .ok(),
        None => None,
    };

    let critical: Option<ThresholdRange> = match args.critical {
        Some(c) => ThresholdRange::from(c.as_str())
            .map_err(|e| exit_with_message(Status::Unknown(UnknownVariant::RangeParseError(c, e))))
            .ok(),
        None => None,
    };

    let thresholds = Thresholds { warning, critical };
    let timeout = Duration::from_millis(args.timeout);

    let socket_type = if args.dgram_socket {
        SocketType::Datagram
    } else {
        SocketType::Raw
    };

    info!("{:<34}{}", "Will check jitter for host:", args.host);
    info!("{:<34}{}", "Aggregation method:", args.aggregation_method);
    info!("{:<34}{}", "Socket type:", socket_type);
    info!("{:<34}{}", "Sample size:", args.samples);
    info!("{:<34}{}ms", "Timeout per ping:", args.timeout);
    info!(
        "{:<34}{}ms",
        "Minimum wait time between pings:", args.min_interval
    );
    info!(
        "{:<34}{}ms",
        "Maximum wait time between pings:", args.max_interval
    );
    info!("{:<34}{}", "Decimal precision:", args.precision);
    info!("{:<34}{:?}", "Warning threshold:", warning);
    info!("{:<34}{:?}", "Critical threshold:", critical);

    let raw_jitter = match get_jitter(
        args.aggregation_method,
        &args.host,
        socket_type,
        args.samples,
        timeout,
        args.min_interval,
        args.max_interval,
    ) {
        Ok(jitter) => jitter,
        Err(e) => exit_with_message(Status::Unknown(UnknownVariant::Error(e))),
    };

    exit_with_message(evaluate_thresholds(
        args.aggregation_method,
        round_jitter(raw_jitter, args.precision),
        &thresholds,
    ))
}
