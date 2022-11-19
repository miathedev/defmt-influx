# defmt-influx

> A tool to decode and pipe logs received through a tcp socket to InfluxDB. This tool is surely not a beauty, but it works. I shared this tool in mind it might be useful for someone else.

This tool automatically retrys a RTT-TCP connection on connection loss. Can be used seamless in development workflow.

This crate is a modified copy of: (https://github.com/Javier-varez/defmt-uart/blob/main/README.md)

This crate is a derived work from the original [defmt](https://github.com/knurling-rs/defmt) project.

[`defmt`]: https://crates.io/crates/defmt

## Installation

run ```cargo install defmt-influx```

## Usage

example: ```defmt-influx --elf "target/thumbv7em-none-eabihf/debug/application" --rtt_port "840"1 --rtt_host "127.0.0.1" --influx_host "http://127.0.0.1:8086" --influx_org "test" --influx_token "pJv-JIBpjYfK-5E1yme8qrlQltU-LgX-xVWxpfsPyyTjFqqpavvItRL9wY8_9QeEWiKzDzClTlzF60e8qwQlfw==" --influx_bucket "Logger" --influx_meassurement "Node1"```

## Support

Original `defmt` work is part of the [Knurling] project, [Ferrous Systems]' effort at
improving tooling used to develop for embedded systems.

If you think this work is useful, consider sponsoring defmt developers via [GitHub
Sponsors].


## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[Knurling]: https://knurling.ferrous-systems.com/
[Ferrous Systems]: https://ferrous-systems.com/
[GitHub Sponsors]: https://github.com/sponsors/knurling-rs
