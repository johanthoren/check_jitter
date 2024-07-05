# check_jitter

This plugin will measure network jitter by sending a number of ICMP pings to a
host and calculate the average jitter based on the differences between
consecutive round trip times.

## Help Text

``` sh
$ check_jitter --help
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


Usage: check_jitter [OPTIONS] --host <HOST>

Options:
  -c, --critical <CRITICAL>          Critical limit for network jitter in milliseconds
  -d, --debug                        Enable debug logging
  -H, --host <HOST>                  Hostname or IP address to ping
  -m, --min-interval <MIN_INTERVAL>  Minimum interval between ping samples in milliseconds [default: 0]
  -M, --max-interval <MAX_INTERVAL>  Maximum interval between ping samples in milliseconds [default: 0]
  -p, --precision <PRECISION>        Precision of the output decimal places [default: 3]
  -s, --samples <SAMPLES>            Number of pings to send [default: 10]
  -t, --timeout <TIMEOUT>            Timeout in milliseconds per individual ping check [default: 1000]
  -w, --warning <WARNING>            Warning limit for network jitter in milliseconds
  -h, --help                         Print help
  -V, --version                      Print version
```

## Installation

Download the latest Linux or Windows binary from [the latest release
page](https://github.com/johanthoren/check_jitter/releases/latest).

Use together with NRPE or similar, preferably with
[Opsview](https://www.itrsgroup.com/products/infrastructure-monitoring).

Note that the plugin requires elevated permissions, so you will have to use
`setuid` or similar.

## License

Copyright © 2024 Johan Thorén <johan@thoren.xyz>

This project is released under the ISC license. See the LICENSE file for more
details.
