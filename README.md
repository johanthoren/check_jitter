# check_jitter

[![crates.io](https://img.shields.io/crates/v/check-jitter.svg)](https://crates.io/crates/check-jitter)
[![Documentation](https://docs.rs/check-jitter/badge.svg)](https://docs.rs/check-jitter)
[![ISC licensed](https://img.shields.io/crates/l/check-jitter.svg)](./LICENSE)

This plugin will measure network jitter by sending a number of ICMP pings to a
host and calculate the average jitter based on the differences between
consecutive round trip times.

## Example Command

``` text
Command:
------------------------------------------------------------------
check_jitter -H 192.168.1.1 -w 10 -c 20 -m 5 -M 50 -s 30 -a median
           |              |     |     |    |     |     |    |
           |              |     |     |    |     |     |    +- Aggregation method: median
           |              |     |     |    |     |     +------ Number of pings to send: 30
           |              |     |     |    |     +------------ Max interval between pings: 50ms
           |              |     |     |    +------------------ Min interval between pings: 5ms
           |              |     |     +----------------------- Critical jitter limit: 20ms
           |              |     +----------------------------- Warning jitter limit: 10ms
           |              +----------------------------------- Host: 192.168.1.1
           +-------------------------------------------------- Command: check_jitter

Output:
------------------------------------------------------------------
OK - Median Jitter: 0.182ms | 'Median Jitter'=0.182ms;0:10;0:20;0
|                                         |       |     |    |  |
|                                         |       |     |    |  +- Minimum possible value (always 0)
|                                         |       |     |    +---- Critical range: 0 to 20ms
|                                         |       |     +--------- Warning range: 0 to 10ms
|                                         |       +--------------- Median jitter: 0.182ms
|                                         +----------------------- Performance data label: 'Median Jitter'
+----------------------------------------------------------------- Status: OK

Explanation of Output:
------------------------------------------------------------------
- Status: OK
  Indicates that the median jitter is within acceptable limits.

- Median Jitter: 0.182ms
  The aggregated median jitter value.

- Performance Data:
  'Median Jitter'=0.182ms;0:10;0:20;0
   |                  |    |    |   |
   |                  |    |    |   +- Minimum possible value (always 0)
   |                  |    |    +----- Critical range: 0 to 20ms
   |                  |    +---------- Warning range: 0 to 10ms
   |                  +--------------- Median jitter value: 0.182ms
   +---------------------------------- Performance data label: 'Median Jitter'

- Unit of Measurement (uom): ms (milliseconds)
  Indicates the measurement unit used in the performance data.

Reason for Status:
----------------------------------------------------------------------
The command did not trigger a warning or critical alert because:
- The median jitter value (0.182ms) is within the defined warning range (0 to 10ms).
- The median jitter value (0.182ms) is within the defined critical range (0 to 20ms).
```

## Help Text

``` text
check_jitter - A monitoring plugin that measures network jitter.

AGGREGATION METHOD

The plugin can aggregate the deltas from multiple samples in the following ways:
- average: the average of all deltas (arithmetic mean) [default]
- median: the median of all deltas
- max: the maximum of all deltas
- min: the minimum of all deltas

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
| 10:              | < 10, (outside {10 .. ‚àû})                       |
+------------------+-------------------------------------------------+
| ~:10             | > 10, (outside the range of {-‚àû .. 10})         |
+------------------+-------------------------------------------------+
| 10:20            | < 10 or > 20, (outside the range of {10 .. 20}) |
+------------------+-------------------------------------------------+
| @10:20           | ‚â• 10 and ‚â§ 20, (inside the range of {10 .. 20}) |
+------------------+-------------------------------------------------+

Usage: check_jitter [OPTIONS] --host <HOST>

Options:
  -a, --aggregation-method <AGGREGATION_METHOD>
          Aggregation method to use for multiple samples [default: average]
  -c, --critical <CRITICAL>
          Critical limit for network jitter in milliseconds
  -D, --dgram-socket
          Use a datagram socket instead of a raw socket (expert option)
  -H, --host <HOST>
          Hostname or IP address to ping
  -m, --min-interval <MIN_INTERVAL>
          Minimum interval between ping samples in milliseconds [default: 0]
  -M, --max-interval <MAX_INTERVAL>
          Maximum interval between ping samples in milliseconds [default: 0]
  -p, --precision <PRECISION>
          Precision of the output decimal places [default: 3]
  -s, --samples <SAMPLES>
          Number of pings to send [default: 10]
  -t, --timeout <TIMEOUT>
          Timeout in milliseconds per individual ping check [default: 1000]
  -w, --warning <WARNING>
          Warning limit for network jitter in milliseconds
  -v, --verbose...
          Enable verbose output. Use multiple times to increase verbosity (e.g. -vvv)
  -h, --help
          Print help
  -V, --version
          Print version
```

## Installation

Download the latest Linux or Windows binary from [the latest release
page](https://github.com/johanthoren/check_jitter/releases/latest).

Use together with NRPE or similar, preferably with
[Opsview](https://www.itrsgroup.com/products/infrastructure-monitoring).

Note that the plugin requires elevated permissions, so you will have to use
`setuid` or `setcap cap_net_raw+ep` on the binary.

## License

Copyright © 2024 Johan Thorén <johan@thoren.xyz>

This project is released under the ISC license. See the LICENSE file for more
details.
