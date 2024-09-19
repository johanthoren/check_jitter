use log::{debug, error, info};
use nagios_range::Error as RangeError;
use nagios_range::NagiosRange as ThresholdRange;
use rand::Rng;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug)]
pub enum SocketType {
    Datagram,
    Raw,
}

impl fmt::Display for SocketType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SocketType::Datagram => write!(f, "Datagram"),
            SocketType::Raw => write!(f, "Raw"),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AggregationMethod {
    Average,
    Median,
    Max,
    Min,
}

impl std::str::FromStr for AggregationMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "average" => Ok(AggregationMethod::Average),
            "avg" => Ok(AggregationMethod::Average),
            "mean" => Ok(AggregationMethod::Average),
            "median" => Ok(AggregationMethod::Median),
            "med" => Ok(AggregationMethod::Median),
            "minimum" => Ok(AggregationMethod::Min),
            "min" => Ok(AggregationMethod::Min),
            "maximum" => Ok(AggregationMethod::Max),
            "max" => Ok(AggregationMethod::Max),
            _ => Err(format!("'{}' is not a valid aggregation method", s)),
        }
    }
}

impl fmt::Display for AggregationMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AggregationMethod::Average => write!(f, "Average"),
            AggregationMethod::Median => write!(f, "Median"),
            AggregationMethod::Max => write!(f, "Max"),
            AggregationMethod::Min => write!(f, "Min"),
        }
    }
}

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

    #[error("DNS resolution error for '{addr}': {error}")]
    DnsResolutionError { addr: String, error: String },

    #[error("The delta count is 0. Cannot calculate jitter.")]
    EmptyDeltas,

    #[error("At least 2 samples are required to calculate jitter, got {0}.")]
    InsufficientSamples(u8),

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

impl From<std::io::Error> for CheckJitterError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => CheckJitterError::PermissionDenied,
            _ => CheckJitterError::PingIoError(err.to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Thresholds {
    pub warning: Option<ThresholdRange>,
    pub critical: Option<ThresholdRange>,
}

#[non_exhaustive]
#[derive(Debug, PartialEq)]
pub enum UnknownVariant {
    Error(CheckJitterError),
    FailedToInitLogger(String),
    InvalidAddr(String),
    InvalidMinMaxInterval(u64, u64),
    ClapError(String),
    NoThresholds,
    RangeParseError(String, RangeError),
    Timeout(Duration),
}

#[derive(Debug, PartialEq)]
pub enum Status<'a> {
    Ok(AggregationMethod, f64, &'a Thresholds),
    Warning(AggregationMethod, f64, &'a Thresholds),
    Critical(AggregationMethod, f64, &'a Thresholds),
    Unknown(UnknownVariant),
}

fn display_string(label: &str, status: &str, uom: &str, f: f64, t: &Thresholds) -> String {
    let min: f64 = 0.0;
    match (t.warning, t.critical) {
        (Some(w), Some(c)) => {
            format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w};{c};{min}")
        }
        (Some(w), None) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w};;{min}"),
        (None, Some(c)) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};;{c};{min}"),
        (None, None) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};;;{min}"),
    }
}

#[cfg(test)]
mod display_string_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_both_thresholds() {
        let thresholds = Thresholds {
            warning: Some(ThresholdRange::from("0:0.5").unwrap()),
            critical: Some(ThresholdRange::from("0:1").unwrap()),
        };

        let expected = "OK - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;0:0.5;0:1;0";
        let actual = display_string("Average Jitter", "OK", "ms", 0.1, &thresholds);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_only_warning() {
        let thresholds = Thresholds {
            warning: Some(ThresholdRange::from("0:0.5").unwrap()),
            critical: None,
        };

        let expected = "OK - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;0:0.5;;0";
        let actual = display_string("Average Jitter", "OK", "ms", 0.1, &thresholds);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_only_critical() {
        let thresholds = Thresholds {
            warning: None,
            critical: Some(ThresholdRange::from("0:0.5").unwrap()),
        };

        let expected = "OK - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;;0:0.5;0";
        let actual = display_string("Average Jitter", "OK", "ms", 0.1, &thresholds);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_no_thresholds() {
        let thresholds = Thresholds {
            warning: None,
            critical: None,
        };

        let expected = "OK - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;;;0";
        let actual = display_string("Average Jitter", "OK", "ms", 0.1, &thresholds);

        assert_eq!(actual, expected);
    }
}

impl fmt::Display for Status<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uom = "ms";
        let label = match self {
            Status::Ok(AggregationMethod::Average, _, _) => "Average Jitter",
            Status::Ok(AggregationMethod::Median, _, _) => "Median Jitter",
            Status::Ok(AggregationMethod::Max, _, _) => "Max Jitter",
            Status::Ok(AggregationMethod::Min, _, _) => "Min Jitter",
            Status::Warning(AggregationMethod::Average, _, _) => "Average Jitter",
            Status::Warning(AggregationMethod::Median, _, _) => "Median Jitter",
            Status::Warning(AggregationMethod::Max, _, _) => "Max Jitter",
            Status::Warning(AggregationMethod::Min, _, _) => "Min Jitter",
            Status::Critical(AggregationMethod::Average, _, _) => "Average Jitter",
            Status::Critical(AggregationMethod::Median, _, _) => "Median Jitter",
            Status::Critical(AggregationMethod::Max, _, _) => "Max Jitter",
            Status::Critical(AggregationMethod::Min, _, _) => "Min Jitter",
            Status::Unknown(_) => "Unknown",
        };

        match self {
            Status::Ok(_, n, t) => {
                write!(f, "{}", display_string(label, "OK", uom, *n, t))
            }
            Status::Warning(_, n, t) => {
                write!(f, "{}", display_string(label, "WARNING", uom, *n, t))
            }
            Status::Critical(_, n, t) => {
                write!(f, "{}", display_string(label, "CRITICAL", uom, *n, t))
            }
            Status::Unknown(UnknownVariant::Error(e)) => {
                write!(f, "UNKNOWN - An error occurred: '{}'", e)
            }
            Status::Unknown(UnknownVariant::FailedToInitLogger(s)) => {
                write!(
                    f,
                    "UNKNOWN - Failed to initialize logger with error: '{}'",
                    s
                )
            }
            Status::Unknown(UnknownVariant::InvalidAddr(s)) => {
                write!(f, "UNKNOWN - Invalid address or hostname: {}", s)
            }
            Status::Unknown(UnknownVariant::InvalidMinMaxInterval(min, max)) => {
                write!(
                    f,
                    "UNKNOWN - Invalid min/max interval: min: {}, max: {}",
                    min, max
                )
            }
            Status::Unknown(UnknownVariant::ClapError(s)) => {
                let trimmed = s.trim_end();
                let without_leading_error = trimmed.trim_start_matches("error: ");
                write!(
                    f,
                    "UNKNOWN - Command line parsing produced an error: {}",
                    without_leading_error,
                )
            }
            Status::Unknown(UnknownVariant::NoThresholds) => {
                write!(
                    f,
                    "UNKNOWN - No thresholds provided. Provide at least one threshold."
                )
            }
            Status::Unknown(UnknownVariant::RangeParseError(s, e)) => {
                write!(
                    f,
                    "UNKNOWN - Unable to parse range '{}' with error: {}",
                    s, e
                )
            }
            Status::Unknown(UnknownVariant::Timeout(d)) => {
                write!(f, "UNKNOWN - Ping timeout occurred after {:?}", d)
            }
        }
    }
}

#[cfg(test)]
mod status_display_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_ok() {
        let t = Thresholds {
            warning: Some(ThresholdRange::from("0:0.5").unwrap()),
            critical: Some(ThresholdRange::from("0:1").unwrap()),
        };
        let status = Status::Ok(AggregationMethod::Average, 0.1, &t);
        let expected = "OK - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;0:0.5;0:1;0";
        let actual = format!("{}", status);

        assert_eq!(actual, expected);
    }

    #[test]
    // The expected value is the same as the previous test, even if the str given to initiate
    // the ThresholdRange is different.
    fn test_with_ok_simple_thresholds() {
        let t = Thresholds {
            warning: Some(ThresholdRange::from("0.5").unwrap()),
            critical: Some(ThresholdRange::from("1").unwrap()),
        };
        let status = Status::Ok(AggregationMethod::Median, 0.1, &t);
        let expected = "OK - Median Jitter: 0.1ms|'Median Jitter'=0.1ms;0:0.5;0:1;0";
        let actual = format!("{}", status);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_warning() {
        let t = Thresholds {
            warning: Some(ThresholdRange::from("0:0.5").unwrap()),
            critical: Some(ThresholdRange::from("0:1").unwrap()),
        };
        let status = Status::Warning(AggregationMethod::Average, 0.1, &t);
        let expected = "WARNING - Average Jitter: 0.1ms|'Average Jitter'=0.1ms;0:0.5;0:1;0";
        let actual = format!("{}", status);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_critical() {
        let t = Thresholds {
            warning: Some(ThresholdRange::from("0:0.5").unwrap()),
            critical: Some(ThresholdRange::from("0:1").unwrap()),
        };
        let status = Status::Critical(AggregationMethod::Max, 0.1, &t);
        let expected = "CRITICAL - Max Jitter: 0.1ms|'Max Jitter'=0.1ms;0:0.5;0:1;0";
        let actual = format!("{}", status);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_error() {
        let status = Status::Unknown(UnknownVariant::Error(CheckJitterError::DnsLookupFailed(
            "example.com".to_string(),
        )));

        let expected = "UNKNOWN - An error occurred: 'DNS Lookup failed for: example.com'";
        let actual = format!("{}", status);

        assert_eq!(actual, expected);
    }
}

impl Status<'_> {
    pub fn to_int(&self) -> i32 {
        match self {
            Status::Ok(_, _, _) => 0,
            Status::Warning(_, _, _) => 1,
            Status::Critical(_, _, _) => 2,
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

#[cfg(test)]
mod abs_diff_duration_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_small_a() {
        let a = Duration::from_nanos(100_000_000);
        let b = Duration::from_nanos(100_100_000);
        let expected = Duration::from_nanos(100_000);
        let actual = abs_diff_duration(a, b);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_small_b() {
        let a = Duration::from_nanos(100_100_000);
        let b = Duration::from_nanos(100_000_000);
        let expected = Duration::from_nanos(100_000);
        let actual = abs_diff_duration(a, b);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_equal_values() {
        let a = Duration::from_nanos(100_000_000);
        let b = Duration::from_nanos(100_000_000);
        let expected = Duration::from_nanos(0);
        let actual = abs_diff_duration(a, b);

        assert_eq!(actual, expected);
    }
}

fn generate_intervals(count: u8, min_interval: u64, max_interval: u64) -> Vec<Duration> {
    if min_interval > max_interval {
        debug!(
            "Invalid min and max interval: min: {}, max: {}. No random intervals will be generated.",
            min_interval, max_interval
        );
        return Vec::new();
    }

    if max_interval == 0 && min_interval == 0 {
        debug!("Min and max interval are both 0. No random intervals will be generated.");
        return Vec::new();
    }

    let mut intervals = Vec::with_capacity(count as usize);

    if max_interval == min_interval {
        debug!(
            "Min and max interval are equal: {}ms. Intervals will not be randomized.",
            min_interval
        );
        for _ in 0..count {
            intervals.push(Duration::from_millis(min_interval));
        }

        debug!("Random intervals: {:?}", intervals);

        return intervals;
    }

    debug!(
        "Generating {} random intervals between {}ms and {}ms...",
        count, min_interval, max_interval
    );

    for _ in 0..count {
        let interval = rand::thread_rng().gen_range(min_interval..=max_interval);
        intervals.push(Duration::from_millis(interval));
    }

    debug!("Random intervals: {:?}", intervals);

    intervals
}

#[cfg(test)]
mod generate_intervals_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_min_max() {
        let count = 10;
        let min_interval = 10;
        let max_interval = 100;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals.len(), count as usize);
        for i in intervals {
            assert!(i >= Duration::from_millis(min_interval));
            assert!(i <= Duration::from_millis(max_interval));
        }
    }

    #[test]
    fn test_with_min_max_equal() {
        let count = 10;
        let min_interval = 10;
        let max_interval = 10;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals.len(), count as usize);
        for i in intervals {
            assert_eq!(i, Duration::from_millis(min_interval));
        }
    }

    #[test]
    fn test_with_min_max_zero() {
        let count = 10;
        let min_interval = 0;
        let max_interval = 0;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals, Vec::<Duration>::new());
        assert!(intervals.is_empty());
    }

    #[test]
    fn test_with_min_max_swapped() {
        let count = 10;
        let min_interval = 100;
        let max_interval = 10;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals, Vec::<Duration>::new());
        assert!(intervals.is_empty());
    }
    #[test]
    fn test_with_zero_count() {
        let count = 0;
        let min_interval = 10;
        let max_interval = 100;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals, Vec::<Duration>::new());
        assert!(intervals.is_empty());
    }

    #[test]
    fn test_with_large_range() {
        let count = 10;
        let min_interval = 1;
        let max_interval = 1_000_000;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals.len(), count as usize);
        for i in intervals {
            assert!(i >= Duration::from_millis(min_interval));
            assert!(i <= Duration::from_millis(max_interval));
        }
    }

    #[test]
    fn test_with_single_interval() {
        let count = 1;
        let min_interval = 10;
        let max_interval = 100;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals.len(), 1);
        assert!(intervals[0] >= Duration::from_millis(min_interval));
        assert!(intervals[0] <= Duration::from_millis(max_interval));
    }

    #[test]
    fn test_with_very_large_intervals() {
        let count = 10;
        let min_interval = u64::MAX - 1_000;
        let max_interval = u64::MAX;
        let intervals = generate_intervals(count, min_interval, max_interval);

        assert_eq!(intervals.len(), count as usize);
        for i in intervals {
            assert!(i >= Duration::from_millis(min_interval));
            assert!(i <= Duration::from_millis(max_interval));
        }
    }
}

fn default_resolver(addr: &str) -> Result<Vec<IpAddr>, CheckJitterError> {
    let addr_with_port = format!("{}:0", addr);
    match addr_with_port.to_socket_addrs() {
        Ok(addrs_iter) => {
            let addrs: Vec<IpAddr> = addrs_iter.map(|sockaddr| sockaddr.ip()).collect();
            if addrs.is_empty() {
                Err(CheckJitterError::DnsLookupFailed(addr.to_string()))
            } else {
                Ok(addrs)
            }
        }
        Err(e) => Err(CheckJitterError::DnsResolutionError {
            addr: addr.to_string(),
            error: e.to_string(),
        }),
    }
}

fn parse_addr_with_resolver<F>(addr: &str, resolver: F) -> Result<Vec<IpAddr>, CheckJitterError>
where
    F: Fn(&str) -> Result<Vec<IpAddr>, CheckJitterError>,
{
    if let Ok(ipv4) = addr.parse::<Ipv4Addr>() {
        return Ok(vec![IpAddr::V4(ipv4)]);
    }
    if let Ok(ipv6) = addr.parse::<Ipv6Addr>() {
        return Ok(vec![IpAddr::V6(ipv6)]);
    }

    // If the address is not an IP address, perform DNS lookup using the provided resolver.
    resolver(addr)
}

fn parse_addr(addr: &str) -> Result<Vec<IpAddr>, CheckJitterError> {
    parse_addr_with_resolver(addr, default_resolver)
}

#[cfg(test)]
mod parse_addr_tests {
    use super::*;

    fn mock_resolver(addr: &str) -> Result<Vec<IpAddr>, CheckJitterError> {
        match addr {
            "localhost" => Ok(vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))]),
            "ipv6-localhost" => Ok(vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]),
            "multi.example.com" => Ok(vec![
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3)),
            ]),
            "unresolved.example.com" => Err(CheckJitterError::DnsLookupFailed(addr.to_string())),
            "error.example.com" => Err(CheckJitterError::DnsResolutionError {
                addr: addr.to_string(),
                error: "mock error".to_string(),
            }),
            _ => Err(CheckJitterError::DnsResolutionError {
                addr: addr.to_string(),
                error: "unknown host".to_string(),
            }),
        }
    }

    #[test]
    fn test_valid_ipv4_address() {
        let addr = "192.168.1.1";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(result, Ok(vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))]));
    }

    #[test]
    fn test_valid_ipv6_address() {
        let addr = "::1";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(result, Ok(vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]));
    }

    #[test]
    fn test_invalid_ip_address() {
        let addr = "999.999.999.999";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(
            result,
            Err(CheckJitterError::DnsResolutionError {
                addr: addr.to_string(),
                error: "unknown host".to_string(),
            })
        );
    }

    #[test]
    fn test_valid_hostname() {
        let addr = "localhost";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(result, Ok(vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))]));
    }

    #[test]
    fn test_valid_ipv6_hostname() {
        let addr = "ipv6-localhost";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(result, Ok(vec![IpAddr::V6(Ipv6Addr::LOCALHOST)]));
    }

    #[test]
    fn test_unresolved_hostname() {
        let addr = "unresolved.example.com";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(
            result,
            Err(CheckJitterError::DnsLookupFailed(addr.to_string()))
        );
    }

    #[test]
    fn test_dns_resolution_error() {
        let addr = "error.example.com";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(
            result,
            Err(CheckJitterError::DnsResolutionError {
                addr: addr.to_string(),
                error: "mock error".to_string(),
            })
        );
    }

    #[test]
    fn test_unknown_hostname() {
        let addr = "unknown.example.com";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(
            result,
            Err(CheckJitterError::DnsResolutionError {
                addr: addr.to_string(),
                error: "unknown host".to_string(),
            })
        );
    }

    #[test]
    fn test_hostname_with_multiple_ips() {
        let addr = "multi.example.com";
        let result = parse_addr_with_resolver(addr, mock_resolver);
        assert_eq!(
            result,
            Ok(vec![
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)),
                IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3)),
            ])
        )
    }
}

fn run_samples(
    ip: IpAddr,
    socket_type: SocketType,
    samples: u8,
    timeout: Duration,
    mut intervals: Vec<Duration>,
) -> Result<Vec<Duration>, CheckJitterError> {
    let ping_function = match socket_type {
        SocketType::Datagram => ping::dgramsock::ping,
        SocketType::Raw => ping::rawsock::ping,
    };
    let mut durations = Vec::<Duration>::with_capacity(samples as usize);
    for i in 0..samples {
        let start = Instant::now();
        match ping_function(ip, Some(timeout), None, None, None, None) {
            Ok(_) => {
                let duration = start.elapsed();
                debug!("Ping round {}, duration: {:?}", i + 1, duration);

                durations.push(duration);

                if let Some(interval) = intervals.pop() {
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

fn get_durations(
    addr: &str,
    socket_type: SocketType,
    samples: u8,
    timeout: Duration,
    min_interval: u64,
    max_interval: u64,
) -> Result<Vec<Duration>, CheckJitterError> {
    // NOTE: Only the first IP address from the list of resolved addresses will be used.
    // TODO: This may change in the future if we decide to ping all resolved addresses by default
    //       or provide an option to do so.
    let ip = match parse_addr(addr)?.first() {
        Some(ip) => *ip,
        None => return Err(CheckJitterError::DnsLookupFailed(addr.to_string())),
    };

    if samples < 2 {
        return Err(CheckJitterError::InsufficientSamples(samples));
    }

    let intervals = generate_intervals(samples - 1, min_interval, max_interval);
    run_samples(ip, socket_type, samples, timeout, intervals)
}

fn calculate_deltas(durations: &[Duration]) -> Result<Vec<Duration>, CheckJitterError> {
    if durations.len() < 2 {
        return Err(CheckJitterError::InsufficientSamples(durations.len() as u8));
    }

    let deltas = durations
        .windows(2)
        .map(|w| abs_diff_duration(w[0], w[1]))
        .collect();

    debug!("Deltas: {:?}", deltas);

    Ok(deltas)
}

#[cfg(test)]
mod calculate_deltas_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_zero_durations() {
        let durations = &[];

        let result = calculate_deltas(durations);

        assert_eq!(result, Err(CheckJitterError::InsufficientSamples(0)));
    }

    #[test]
    fn test_with_one_duration() {
        let durations = &[Duration::from_nanos(100_000_000)];

        let result = calculate_deltas(durations);

        assert_eq!(result, Err(CheckJitterError::InsufficientSamples(1)));
    }

    #[test]
    fn test_with_two_durations() {
        let durations = &[
            Duration::from_nanos(100_000_000),
            Duration::from_nanos(100_100_000),
        ];

        let expected_deltas = vec![Duration::from_nanos(100_000)];

        let deltas = calculate_deltas(durations).unwrap();

        assert_eq!(deltas, expected_deltas);
    }

    #[test]
    fn test_with_simple_durations() {
        let durations = &[
            Duration::from_nanos(100_000_000),
            Duration::from_nanos(100_100_000),
            Duration::from_nanos(100_200_000),
            Duration::from_nanos(100_300_000),
        ];

        let expected_deltas = &[
            Duration::from_nanos(100_000),
            Duration::from_nanos(100_000),
            Duration::from_nanos(100_000),
        ];

        let deltas = calculate_deltas(durations).unwrap();

        assert_eq!(deltas, expected_deltas);
    }

    #[test]
    fn test_with_irregular_durations() {
        let durations = &[
            Duration::from_nanos(100_000_000),
            Duration::from_nanos(100_101_200),
            Duration::from_nanos(101_200_030),
            Duration::from_nanos(100_310_900),
        ];

        let expected_deltas = &[
            Duration::from_nanos(101_200),
            Duration::from_nanos(1_098_830),
            Duration::from_nanos(889_130),
        ];

        let deltas = calculate_deltas(durations).unwrap();
        assert_eq!(deltas, expected_deltas);
    }
}

fn calculate_avg_jitter(deltas: Vec<Duration>) -> f64 {
    let total_jitter = deltas.iter().sum::<Duration>();
    debug!("Sum of deltas: {:?}", total_jitter);

    let avg_jitter = total_jitter / deltas.len() as u32;
    debug!("Average jitter duration: {:?}", avg_jitter);

    let average_float = avg_jitter.as_secs_f64() * 1_000.0;
    debug!("Average jitter as f64: {:?}", average_float);

    average_float
}

fn calculate_median_jitter(deltas: Vec<Duration>) -> f64 {
    let mut sorted_deltas = deltas.clone();
    sorted_deltas.sort();
    debug!("Sorted deltas: {:?}", sorted_deltas);

    let len = sorted_deltas.len();
    debug!("Number of deltas: {}", len);

    let median_float: f64 = if len % 2 == 0 {
        let mid = len / 2;
        let mid_1 = mid - 1;
        let mid_2 = mid;
        let dur_1 = sorted_deltas[mid_1].as_secs_f64() * 1_000.0;
        let dur_2 = sorted_deltas[mid_2].as_secs_f64() * 1_000.0;
        (dur_1 + dur_2) / 2.0
    } else {
        let mid = len / 2;
        sorted_deltas[mid].as_secs_f64() * 1_000.0
    };
    debug!("Median jitter as f64: {:?}", median_float);

    median_float
}

fn calculate_max_jitter(deltas: Vec<Duration>) -> Result<f64, CheckJitterError> {
    let max = deltas.iter().max().ok_or(CheckJitterError::EmptyDeltas)?;
    debug!("Max jitter: {:?}", max);
    let max_float = max.as_secs_f64() * 1_000.0;
    debug!("Max jitter as f64: {:?}", max_float);

    Ok(max_float)
}

fn calculate_min_jitter(deltas: Vec<Duration>) -> Result<f64, CheckJitterError> {
    let min = deltas.iter().min().ok_or(CheckJitterError::EmptyDeltas)?;
    debug!("Min jitter: {:?}", min);
    let min_float = min.as_secs_f64() * 1_000.0;
    debug!("Min jitter as f64: {:?}", min_float);

    Ok(min_float)
}

/// Round the jitter to the specified precision.
pub fn round_jitter(j: f64, precision: u8) -> f64 {
    let factor = 10f64.powi(precision as i32);
    let rounded_jitter = (j * factor).round() / factor;
    debug!("jitter as rounded f64: {:?}", rounded_jitter);

    rounded_jitter
}

#[cfg(test)]
mod calculate_rounded_jitter_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_with_simple_durations() {
        let simple_durations = vec![
            Duration::from_nanos(100_000_000),
            Duration::from_nanos(100_100_000),
            Duration::from_nanos(100_200_000),
            Duration::from_nanos(100_300_000),
            Duration::from_nanos(100_400_000),
            Duration::from_nanos(100_500_000),
            Duration::from_nanos(100_600_000),
            Duration::from_nanos(100_700_000),
            Duration::from_nanos(100_800_000),
            Duration::from_nanos(100_900_000),
        ];

        let expected_average_jitter = 0.1;
        let expected_median_jitter = 0.1;
        let expected_max_jitter = 0.1;
        let expected_min_jitter = 0.1;
        let deltas = calculate_deltas(&simple_durations).unwrap();
        println!("{:#?}", deltas.clone());
        let average_jitter = calculate_avg_jitter(deltas.clone());
        let median_jitter = calculate_median_jitter(deltas.clone());
        let max_jitter = calculate_max_jitter(deltas.clone()).unwrap();
        let min_jitter = calculate_min_jitter(deltas).unwrap();
        let rounded_average_jitter = round_jitter(average_jitter, 3);
        let rounded_median_jitter = round_jitter(median_jitter, 3);
        let rounded_max_jitter = round_jitter(max_jitter, 3);
        let rounded_min_jitter = round_jitter(min_jitter, 3);

        assert_eq!(rounded_average_jitter, expected_average_jitter);
        assert_eq!(rounded_median_jitter, expected_median_jitter);
        assert_eq!(rounded_max_jitter, expected_max_jitter);
        assert_eq!(rounded_min_jitter, expected_min_jitter);
    }

    #[test]
    fn test_with_even_number_of_irregular_durations() {
        let irregular_durations = vec![
            Duration::from_nanos(270_279_792),
            Duration::from_nanos(270_400_049),
            Duration::from_nanos(270_242_514),
            Duration::from_nanos(269_988_869),
            Duration::from_nanos(270_157_314),
            Duration::from_nanos(270_096_136),
            Duration::from_nanos(270_105_637),
            Duration::from_nanos(270_003_857),
            Duration::from_nanos(270_192_099),
            Duration::from_nanos(270_035_557),
        ];

        let expected_average_jitter = 0.135_236;
        let expected_median_jitter = 0.156_542;
        let expected_max_jitter = 0.253_645;
        let expected_min_jitter = 0.009_501;
        let deltas = calculate_deltas(&irregular_durations).unwrap();
        println!("{:#?}", deltas.clone());
        let average_jitter = calculate_avg_jitter(deltas.clone());
        let median_jitter = calculate_median_jitter(deltas.clone());
        let max_jitter = calculate_max_jitter(deltas.clone()).unwrap();
        let min_jitter = calculate_min_jitter(deltas).unwrap();
        let rounded_average_jitter = round_jitter(average_jitter, 6);
        let rounded_median_jitter = round_jitter(median_jitter, 6);
        let rounded_max_jitter = round_jitter(max_jitter, 6);
        let rounded_min_jitter = round_jitter(min_jitter, 6);

        assert_eq!(rounded_average_jitter, expected_average_jitter);
        assert_eq!(rounded_median_jitter, expected_median_jitter);
        assert_eq!(rounded_max_jitter, expected_max_jitter);
        assert_eq!(rounded_min_jitter, expected_min_jitter);
    }

    #[test]
    fn test_with_uneven_number_of_irregular_durations() {
        let irregular_durations = vec![
            Duration::from_nanos(270_279_792),
            Duration::from_nanos(270_400_049),
            Duration::from_nanos(270_242_514),
            Duration::from_nanos(269_988_869),
            Duration::from_nanos(270_157_314),
            Duration::from_nanos(270_096_136),
            Duration::from_nanos(270_105_637),
            Duration::from_nanos(270_003_857),
            Duration::from_nanos(270_192_099),
        ];

        let expected_average_jitter = 0.132_572;
        let expected_median_jitter = 0.138_896;
        let expected_max_jitter = 0.253_645;
        let expected_min_jitter = 0.009_501;
        let deltas = calculate_deltas(&irregular_durations).unwrap();
        println!("{:#?}", deltas.clone());
        let average_jitter = calculate_avg_jitter(deltas.clone());
        let median_jitter = calculate_median_jitter(deltas.clone());
        let max_jitter = calculate_max_jitter(deltas.clone()).unwrap();
        let min_jitter = calculate_min_jitter(deltas).unwrap();
        let rounded_average_jitter = round_jitter(average_jitter, 6);
        let rounded_median_jitter = round_jitter(median_jitter, 6);
        let rounded_max_jitter = round_jitter(max_jitter, 6);
        let rounded_min_jitter = round_jitter(min_jitter, 6);

        assert_eq!(rounded_average_jitter, expected_average_jitter);
        assert_eq!(rounded_median_jitter, expected_median_jitter);
        assert_eq!(rounded_max_jitter, expected_max_jitter);
        assert_eq!(rounded_min_jitter, expected_min_jitter);
    }
}

/// Get and calculate the aggregated jitter to an IP address or hostname.
///
/// This function will perform a DNS lookup if a hostname is provided and then use that IP address
/// to ping the target. The function will then calculate the aggregated value based on the
/// aggregation method passed as an argument. This value will then be rounded to the specified
/// decimal.
///
/// Note that opening a raw socket requires root privileges on Unix-like systems.
///
/// # Arguments
/// * `aggr_method` - The aggregation method to use.
/// * `addr` - The IP address or hostname to ping.
/// * `socket_type` - The type of socket to use for the ping.
/// * `samples` - The number of samples (pings) to take.
/// * `timeout` - The timeout for each ping.
/// * `min_interval` - The minimum interval between pings in milliseconds.
/// * `max_interval` - The maximum interval between pings in milliseconds.
///
/// # Returns
/// The aggregated jitter in milliseconds as a floating point number rounded to the
/// specified decimal.
///
/// # Example
/// ```rust,no_run
/// // This example will not run because it requires root privileges.
/// use check_jitter::{get_jitter, CheckJitterError, AggregationMethod, SocketType};
/// use std::time::Duration;
///
/// let jitter = get_jitter(
///     AggregationMethod::Average, // aggr_method
///     "192.168.1.1",              // addr
///     SocketType::Raw,            // socket_type
///     10,                         // samples
///     Duration::from_secs(1),     // timeout
///     10,                         // min_interval
///     100).unwrap();              // max_interval
/// println!("Average jitter: {}ms", jitter);
/// ```
pub fn get_jitter(
    aggr_method: AggregationMethod,
    addr: &str,
    socket_type: SocketType,
    samples: u8,
    timeout: Duration,
    min_interval: u64,
    max_interval: u64,
) -> Result<f64, CheckJitterError> {
    let durations = get_durations(
        addr,
        socket_type,
        samples,
        timeout,
        min_interval,
        max_interval,
    )?;
    let deltas = calculate_deltas(&durations)?;
    match aggr_method {
        AggregationMethod::Average => Ok(calculate_avg_jitter(deltas)),
        AggregationMethod::Median => Ok(calculate_median_jitter(deltas)),
        AggregationMethod::Max => calculate_max_jitter(deltas),
        AggregationMethod::Min => calculate_min_jitter(deltas),
    }
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
/// use check_jitter::{evaluate_thresholds, AggregationMethod, Thresholds, Status};
/// use nagios_range::NagiosRange as ThresholdRange;
/// use std::time::Duration;
///
/// let jitter = 0.1;
/// let thresholds = Thresholds {
///     warning: Some(ThresholdRange::from("0:0.5").unwrap()),
///     critical: Some(ThresholdRange::from("0:1").unwrap()),
/// };
///
/// let status = evaluate_thresholds(AggregationMethod::Average, jitter, &thresholds);
///
/// match status {
///     Status::Ok(_, _, _) => println!("Jitter is OK"),
///     Status::Warning(_, _, _) => println!("Jitter is warning"),
///     Status::Critical(_, _, _) => println!("Jitter is critical"),
///     Status::Unknown(_) => println!("Unknown status"),
/// }
/// ```
pub fn evaluate_thresholds(
    aggr_method: AggregationMethod,
    value: f64,
    thresholds: &Thresholds,
) -> Status {
    info!("Evaluating jitter: {:?}", value);
    if let Some(c) = thresholds.critical {
        info!("Checking critical threshold: {:?}", c);
        if c.check(value) {
            info!("Jitter is critical: {:?}", value);
            return Status::Critical(aggr_method, value, thresholds);
        } else {
            info!("Jitter is not critical: {:?}", value);
        }
    } else {
        info!("No critical threshold provided");
    }

    if let Some(w) = thresholds.warning {
        info!("Checking warning threshold: {:?}", w);
        if w.check(value) {
            info!("Jitter is warning: {:?}", value);
            return Status::Warning(aggr_method, value, thresholds);
        } else {
            info!("Jitter is not warning: {:?}", value);
        }
    } else {
        info!("No warning threshold provided");
    }

    Status::Ok(aggr_method, value, thresholds)
}
