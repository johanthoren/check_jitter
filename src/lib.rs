use log::{debug, error};
use nagios_range::NagiosRange;
use rand::Rng;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::thread;
use std::time::{Duration, SystemTime};
use thiserror::Error;

#[derive(Debug)]
pub struct PingErrorWrapper(ping::Error);

impl PartialEq for PingErrorWrapper {
    fn eq(&self, other: &Self) -> bool {
        format!("{:?}", self.0) == format!("{:?}", other.0)
    }
}

impl Eq for PingErrorWrapper {}

impl fmt::Display for PingErrorWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::error::Error for PingErrorWrapper {}

#[non_exhaustive]
#[derive(Error, Debug, Eq, PartialEq)]
pub enum CheckJitterError {
    #[error("DNS Lookup failed for: {0}")]
    DnsLookupFailed(String),

    #[error("Invalid IP: {0}")]
    InvalidIP(String),

    #[error("Ping failed because of insufficient permissions")]
    PermissionDenied,

    #[error("Ping failed with error: {0}")]
    PingError(PingErrorWrapper),

    #[error("Ping failed with IO error: {0}")]
    PingIoError(String),

    #[error("Ping timed out after: {0}ms")]
    Timeout(String),

    #[error("Unable to parse hostname: {0}")]
    UrlParseError(url::ParseError),
}

impl From<url::ParseError> for CheckJitterError {
    fn from(err: url::ParseError) -> Self {
        CheckJitterError::UrlParseError(err)
    }
}

impl From<ping::Error> for CheckJitterError {
    fn from(err: ping::Error) -> Self {
        CheckJitterError::PingError(PingErrorWrapper(err))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thresholds {
    pub warning: Option<NagiosRange>,
    pub critical: Option<NagiosRange>,
}

#[non_exhaustive]
#[derive(Debug, PartialEq)]
pub enum UnkownVariant {
    Error(CheckJitterError),
    InvalidAddr(String),
    InvalidMinMaxInterval(u64, u64),
    NoThresholds,
    RangeParseError(String, nagios_range::Error),
    Timeout(Duration),
}

#[derive(Debug, PartialEq)]
pub enum Status<'a> {
    Ok(f64, &'a Thresholds),
    Warning(f64, &'a Thresholds),
    Critical(f64, &'a Thresholds),
    Unknown(UnkownVariant),
}

fn display_string(status: &str, uom: &str, f: f64, t: &Thresholds) -> String {
    let label = "Average Jitter";
    match (t.warning, t.critical) {
        (Some(w), Some(c)) => {
            format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w};{c}")
        }
        (Some(w), None) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w}"),
        (None, Some(c)) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};;{c}"),
        (None, None) => format!("{status} - {label}: {f}{uom}"),
    }
}

impl fmt::Display for Status<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Status::Ok(n, t) => write!(f, "{}", display_string("OK", "ms", *n, t)),
            Status::Warning(n, t) => write!(f, "{}", display_string("WARNING", "ms", *n, t)),
            Status::Critical(n, t) => write!(f, "{}", display_string("CRITICAL", "ms", *n, t)),
            Status::Unknown(UnkownVariant::Error(e)) => {
                write!(f, "UNKNOWN - An error occurred: '{}'", e)
            }
            Status::Unknown(UnkownVariant::InvalidAddr(s)) => {
                write!(f, "UNKNOWN - Invalid address or hostname: {}", s)
            }
            Status::Unknown(UnkownVariant::InvalidMinMaxInterval(min, max)) => {
                write!(
                    f,
                    "UNKNOWN - Invalid min/max interval: min: {}, max: {}",
                    min, max
                )
            }
            Status::Unknown(UnkownVariant::NoThresholds) => {
                write!(
                    f,
                    "UNKNOWN - No thresholds provided. Provide at least one threshold."
                )
            }
            Status::Unknown(UnkownVariant::RangeParseError(s, e)) => {
                write!(
                    f,
                    "UNKNOWN - Unable to parse range '{}' with error: {}",
                    s, e
                )
            }
            Status::Unknown(UnkownVariant::Timeout(d)) => {
                write!(f, "UNKNOWN - Ping timeout occurred after {:?}", d)
            }
        }
    }
}

impl Status<'_> {
    pub fn to_int(&self) -> i32 {
        match self {
            Status::Ok(_, _) => 0,
            Status::Warning(_, _) => 1,
            Status::Critical(_, _) => 2,
            Status::Unknown(_) => 3,
        }
    }
}

fn abs_diff_duration(a: Duration, b: Duration) -> Duration {
    if a > b {
        a - b
    } else {
        b - a
    }
}

fn generate_rnd_intervals(count: usize, min_interval: u64, max_interval: u64) -> Vec<Duration> {
    let mut rnd_intervals = Vec::<Duration>::with_capacity(count);

    if max_interval != 0 && min_interval <= max_interval {
        debug!(
            "Generating {} random intervals between {}ms and {}ms...",
            count, min_interval, max_interval
        );

        for _ in 0..count {
            let interval = rand::thread_rng().gen_range(min_interval..=max_interval);
            rnd_intervals.push(Duration::from_millis(interval));
        }

        debug!("Random intervals: {:?}", rnd_intervals);
    }

    rnd_intervals
}

fn get_durations(
    addr: &str,
    samples: usize,
    timeout: Duration,
    min_interval: u64,
    max_interval: u64,
) -> Result<Vec<Duration>, CheckJitterError> {
    let ip = if let Ok(ipv4) = addr.parse::<Ipv4Addr>() {
        IpAddr::V4(ipv4)
    } else if let Ok(ipv6) = addr.parse::<Ipv6Addr>() {
        IpAddr::V6(ipv6)
    } else {
        // Perform DNS lookup
        // TODO: Don't use unwrap().
        match (addr, 0).to_socket_addrs().unwrap().next() {
            Some(socket_addr) => socket_addr.ip(),
            None => return Err(CheckJitterError::DnsLookupFailed(addr.to_string())),
        }
    };

    let mut durations = Vec::<Duration>::with_capacity(samples);
    let mut rnd_intervals = generate_rnd_intervals(samples - 1, min_interval, max_interval);

    for i in 0..samples {
        let start = SystemTime::now();
        debug!("Ping round {}, start time: {:?}", i + 1, start);
        match ping::ping(ip, Some(timeout), None, None, None, None) {
            Ok(_) => {
                let end = SystemTime::now();
                debug!("Ping round {}, end time: {:?}", i + 1, end);

                let duration = end.duration_since(start).unwrap();
                debug!("Ping round {}, duration: {:?}", i + 1, duration);

                durations.push(end.duration_since(start).unwrap());

                if let Some(interval) = rnd_intervals.pop() {
                    debug!("Sleeping for {:?}...", interval);
                    thread::sleep(interval);
                };
            }
            Err(e) => {
                if let ping::Error::IoError { error } = &e {
                    match error.kind() {
                        std::io::ErrorKind::PermissionDenied => {
                            return Err(CheckJitterError::PermissionDenied);
                        }
                        std::io::ErrorKind::WouldBlock => {
                            return Err(CheckJitterError::Timeout(timeout.as_millis().to_string()));
                        }
                        _ => {
                            return Err(CheckJitterError::PingIoError(error.to_string()));
                        }
                    }
                }
                return Err(CheckJitterError::PingError(PingErrorWrapper(e)));
            }
        };
    }

    debug!("Ping durations: {:?}", durations);

    Ok(durations)
}

fn calculate_deltas(durations: Vec<Duration>) -> Result<Vec<Duration>, CheckJitterError> {
    let delta_count: usize = durations.len() - 1;
    let mut deltas = Vec::<Duration>::with_capacity(delta_count);

    for i in 1..durations.len() {
        let d = abs_diff_duration(durations[i], durations[i - 1]);
        deltas.push(d);
    }

    debug!("Deltas: {:?}", deltas);

    Ok(deltas)
}

fn calculate_avg_jitter(deltas: Vec<Duration>) -> Result<f64, CheckJitterError> {
    let total_jitter = deltas.iter().sum::<Duration>();
    debug!("Sum of deltas: {:?}", total_jitter);

    let avg_jitter = total_jitter / deltas.len() as u32;
    debug!("Average jitter duration: {:?}", avg_jitter);

    let jitter_float = avg_jitter.as_secs_f64() * 1000.0;
    debug!("jitter as f64: {:?}", jitter_float);

    Ok(jitter_float)
}

fn round_jitter(j: f64, precision: u8) -> Result<f64, CheckJitterError> {
    let factor = 10f64.powi(precision as i32);
    let rounded_avg_jitter = (j * factor).round() / factor;
    debug!("jitter as rounded f64: {:?}", rounded_avg_jitter);

    Ok(rounded_avg_jitter)
}

/// Get and calculate the average jitter to an IP address or hostname.
///
/// This function will perform a DNS lookup if a hostname is provided and then use that IP address
/// to ping the target. The function will then calculate the average difference in duration between
/// the pings.
///
/// The average rounded jitter will then be calculated using these deltas.
///
/// Note that opening a raw socket requires root privileges on Unix-like systems.
///
/// # Arguments
/// * `addr` - The IP address or hostname to ping.
/// * `samples` - The number of samples (pings) to take.
/// * `timeout` - The timeout for each ping.
/// * `precision` - The number of decimal places to round the jitter to.
/// * `min_interval` - The minimum interval between pings in milliseconds.
/// * `max_interval` - The maximum interval between pings in milliseconds.
///
/// # Returns
/// The average jitter in milliseconds as a floating point number rounded to the specified decimal.
///
/// # Example
/// ```rust,no_run // This example will not run because it requires root privileges.
/// use check_jitter::{get_jitter, CheckJitterError};
/// use std::time::Duration;
///
/// fn main() -> Result<(), CheckJitterError> {
///    let jitter = get_jitter("192.168.1.1", 10, Duration::from_secs(1), 3, 10, 100)?;
///    println!("Average jitter: {}ms", jitter);
///    Ok(())
/// }
/// ```
pub fn get_jitter(
    addr: &str,
    samples: usize,
    timeout: Duration,
    precision: u8,
    min_interval: u64,
    max_interval: u64,
) -> Result<f64, CheckJitterError> {
    let durations = get_durations(addr, samples, timeout, min_interval, max_interval)?;
    let deltas = calculate_deltas(durations)?;
    let avg_jitter = calculate_avg_jitter(deltas)?;
    round_jitter(avg_jitter, precision)
}

/// Evaluate the jitter against the thresholds and return the appropriate status.
///
/// This function will evaluate the jitter against the provided thresholds and return the
/// appropriate status. It will match against the critical threshold first and then the warning
/// threshold, returning the first match or `Status::Ok` if no thresholds are matched.
///
/// # Arguments
/// * `jitter` - The jitter to evaluate as a 64 bit floating point number.
/// * `thresholds` - A reference to the `Thresholds` to evaluate against.
///
/// # Returns
/// The `Status` of the jitter against the thresholds.
///
/// # Example
/// ```rust
/// use check_jitter::{evaluate_thresholds, Thresholds, Status};
/// use nagios_range::NagiosRange;
/// use std::time::Duration;
///
/// fn main() {
///    let jitter = 0.1;
///    let thresholds = Thresholds {
///        warning: Some(NagiosRange::from("0:0.5").unwrap()),
///        critical: Some(NagiosRange::from("0:1").unwrap()),
///    };
///
///    let status = evaluate_thresholds(jitter, &thresholds);
///
///    match status {
///        Status::Ok(_, _) => println!("Jitter is OK"),
///        Status::Warning(_, _) => println!("Jitter is warning"),
///        Status::Critical(_, _) => println!("Jitter is critical"),
///        Status::Unknown(_) => println!("Unknown status"),
///    }
/// }
/// ```
pub fn evaluate_thresholds(jitter: f64, thresholds: &Thresholds) -> Status {
    if let Some(c) = thresholds.critical {
        debug!("Checking critical threshold: {:?}", c);
        if c.check(jitter) {
            debug!("Jitter is critical: {:?}", jitter);
            return Status::Critical(jitter, thresholds);
        } else {
            debug!("Jitter is not critical: {:?}", jitter);
        }
    } else {
        debug!("No critical threshold provided");
    }

    if let Some(w) = thresholds.warning {
        debug!("Checking warning threshold: {:?}", w);
        if w.check(jitter) {
            debug!("Jitter is warning: {:?}", jitter);
            return Status::Warning(jitter, thresholds);
        } else {
            debug!("Jitter is not warning: {:?}", jitter);
        }
    } else {
        debug!("No warning threshold provided");
    }

    Status::Ok(jitter, thresholds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn durations_1() -> Vec<Duration> {
        vec![
            Duration::from_nanos(100000000),
            Duration::from_nanos(100100000),
            Duration::from_nanos(100200000),
            Duration::from_nanos(100300000),
            Duration::from_nanos(100400000),
            Duration::from_nanos(100500000),
            Duration::from_nanos(100600000),
            Duration::from_nanos(100700000),
            Duration::from_nanos(100800000),
            Duration::from_nanos(100900000),
        ]
    }

    #[test]
    fn test_calculate_deltas_1() {
        let expected_jitter = 0.1;
        let deltas = calculate_deltas(durations_1()).unwrap();
        let avg_jitter = calculate_avg_jitter(deltas).unwrap();
        let rounded_avg_jitter = round_jitter(avg_jitter, 3).unwrap();

        assert_eq!(rounded_avg_jitter, expected_jitter);
    }

    fn durations_2() -> Vec<Duration> {
        vec![
            Duration::from_nanos(270279792),
            Duration::from_nanos(270400049),
            Duration::from_nanos(270242514),
            Duration::from_nanos(269988869),
            Duration::from_nanos(270157314),
            Duration::from_nanos(270096136),
            Duration::from_nanos(270105637),
            Duration::from_nanos(270003857),
            Duration::from_nanos(270192099),
            Duration::from_nanos(270035557),
        ]
    }

    #[test]
    fn test_calculate_deltas_2() {
        let expected_jitter = 0.135236;
        let deltas = calculate_deltas(durations_2()).unwrap();
        let avg_jitter = calculate_avg_jitter(deltas).unwrap();
        let rounded_avg_jitter = round_jitter(avg_jitter, 6).unwrap();

        assert_eq!(rounded_avg_jitter, expected_jitter);
    }
}
