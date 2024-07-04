use log::{debug, error};
use nagios_range::NagiosRange;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::time::{Duration, SystemTime};
use thiserror::Error;

#[derive(Debug)]
pub struct PingErrorWrapper(pub ping::Error);

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

    #[error("Ping failed because of insufficient permissions")]
    PermissionDenied,

    #[error("Ping timed out after: {0}ms")]
    Timeout(String),

    #[error("Ping failed with IO error: {0}")]
    PingIoError(String),

    #[error("Invalid IP: {0}")]
    InvalidIP(String),

    #[error("Ping failed with error: {0}")]
    PingError(PingErrorWrapper),

    #[error("Unable to parse host: {0}")]
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
    InvalidHost(String),
    NoThresholds,
    Error(CheckJitterError),
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
            format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w}{uom};{c}{uom}")
        }
        (Some(w), None) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};{w}{uom};"),
        (None, Some(c)) => format!("{status} - {label}: {f}{uom}|'{label}'={f}{uom};;{c}{uom}"),
        (None, None) => format!("{status} - {label}: {f}{uom}"),
    }
}

impl fmt::Display for Status<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Status::Ok(n, t) => write!(f, "{}", display_string("OK", "ms", *n, t)),
            Status::Warning(n, t) => write!(f, "{}", display_string("WARNING", "ms", *n, t)),
            Status::Critical(n, t) => write!(f, "{}", display_string("CRITICAL", "ms", *n, t)),
            Status::Unknown(UnkownVariant::InvalidHost(s)) => {
                write!(f, "UNKNOWN - Invalid host: {}", s)
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
            Status::Unknown(UnkownVariant::Error(e)) => {
                write!(f, "UNKNOWN - An error occurred: '{}'", e)
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

fn get_durations(
    host: &str,
    samples: u32,
    timeout: Duration,
) -> Result<Vec<Duration>, CheckJitterError> {
    let ip = if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
        IpAddr::V4(ipv4)
    } else if let Ok(ipv6) = host.parse::<Ipv6Addr>() {
        IpAddr::V6(ipv6)
    } else {
        // Perform DNS lookup
        // TODO: Don't use unwrap().
        match (host, 0).to_socket_addrs().unwrap().next() {
            Some(socket_addr) => socket_addr.ip(),
            None => return Err(CheckJitterError::DnsLookupFailed(host.to_string())),
        }
    };

    let mut durations = Vec::<Duration>::with_capacity(samples as usize);

    for _ in 0..samples {
        let start = SystemTime::now();
        match ping::ping(ip, Some(timeout), None, None, None, None) {
            Ok(_) => {
                let end = SystemTime::now();
                durations.push(end.duration_since(start).unwrap());
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

pub fn round_jitter(j: f64, precision: u8) -> Result<f64, CheckJitterError> {
    let factor = 10f64.powi(precision as i32);
    let rounded_avg_jitter = (j * factor).round() / factor;
    debug!("jitter as rounded f64: {:?}", rounded_avg_jitter);

    Ok(rounded_avg_jitter)
}

pub fn get_jitter(
    host: &str,
    samples: u32,
    timeout: Duration,
    precision: u8,
) -> Result<f64, CheckJitterError> {
    let durations = get_durations(host, samples, timeout)?;
    let deltas = calculate_deltas(durations)?;
    let avg_jitter = calculate_avg_jitter(deltas)?;
    round_jitter(avg_jitter, precision)
}

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
