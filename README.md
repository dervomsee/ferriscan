# Ferriscan

Ferriscan is an independent, open-source Rust application for scanning with ScanSnap devices without
ScanSnap Home. Wi-Fi is the first implementation milestone and primary transport; USB may follow as a
separate adapter behind the same scanner interfaces. The initial model drivers target the ScanSnap
iX1500 and iX500, with separate modules allowing the model list to grow. The desktop GUI will use Slint.
Linux is the primary platform, with Windows and macOS as optional best-effort targets.

> **Early development.** Discovery, registration, and color scanning have been validated live on one iX500
> and are covered by mock-based tests. Notifications, heartbeat, image post-processing, and grayscale
> conversion are not implemented yet. Support for other models, USB, and platforms is not claimed.

> **Unofficial project.** Ferriscan is not affiliated with, endorsed by, or supported by PFU, Ricoh, or
> Fujitsu. ScanSnap is a trademark of PFU Limited. Use the software at your own risk. See the
> [disclaimer](docs/disclaimer.md).

## Documentation

- [Protocol overview](docs/protocol.md), with detailed write-ups for the
  [iX1500](docs/protocol-ix1500.md) and [iX500](docs/protocol-ix500.md)
- [Model and transport status](docs/models.md), including the process for adding scanner drivers
- [Contributor and agent instructions](AGENTS.md)
- [Development tools and MCP integration](docs/development.md)
- [Architecture decisions](docs/adr/README.md)
- [Changelog](CHANGELOG.md)
- [Reverse-engineering disclaimer](docs/disclaimer.md)

The protocol documentation is adapted from [ScanSnack](https://github.com/elte0/scansnack). Its
observations and live tests are attributed to ScanSnack and are not claims about the current Rust
implementation. See [NOTICE.md](NOTICE.md).

## Current iX500 commands

```sh
cargo run -- ix500 discover
cargo run -- ix500 discover --target <scanner-ip>
cargo run -- ix500 register --ip <scanner-ip> --password <scanner-wifi-password>
cargo run -- ix500 info --ip <scanner-ip>
cargo run -- ix500 scan --ip <scanner-ip> --dpi 300 --duplex --out scans/
```

`discover` first probes directed and limited broadcast addresses. If a scanner remains silent, it sends
concurrent unicast probes across each local subnet, bounded to a /24 per interface, so no target IP is
required on networks where broadcast is filtered or ignored. A directly supplied `--target` disables this
search and uses only that address.

`scan`, `info`, and `register` probe the supplied IP first. If that unicast probe goes unanswered — which
happens for a short period after a session ends — they fall back to the same broadcast and local-subnet
search. Linux determines the MAC address of the interface that routes to the scanner; `--mac` overrides it
when necessary.

`register` performs the state-changing 0x11 registration and stores the identity (see below). Without a
`--password` it uses a stored password and otherwise the factory default. Do not run registration while
ScanSnap Home or another client is using the scanner.

### Identity and password

The iX500 validates the scanner Wi-Fi password on every registration, so Ferriscan remembers it in
`$XDG_CONFIG_HOME/ferriscan/ix500.conf` (default `~/.config/ferriscan/ix500.conf`). `register` stores the
password and the paired host ID; later `scan` and `info` runs reuse them and need no password.

Credential precedence is `--password`, then `FERRISCAN_IX500_PASSWORD`, then the stored password, then the
factory default (the last four serial-number digits). `--password ''` means the scanner has no password.
`--identity <path>` selects another file and `--no-save` disables writing it.

> **The identity file contains the scanner Wi-Fi password in plain text.** It is created with mode `0600`.
> Do not commit it and do not share it. Passing the password as a command-line argument can also expose it
> in the process list; prefer the stored file or the environment variable.

`info` registers this host as well, because the iX500 closes the control connection for the name query
(0x13) until a host is registered.

### Scanning

`scan` registers the host, acquires sheets, releases the scan session with D6, and writes
`pageNNN_front.jpg` plus optional `pageNNN_back.jpg` files. Supported resolutions are 150, 200, 300, and
600 dpi; page sizes are `auto`, `a4`, `letter`, `legal`, `b5`, `a5`, and `b6`. Multi-feed detection defaults
to `ultrasonic` and can be disabled with `--multifeed off`.

Images are currently saved in the scanner's raw orientation and full scan area: they are not rotated,
cropped, deskewed, or converted to grayscale. The iX500 always reads both sides; without `--duplex` the
back side is discarded.

Every scan runs until the feeder is empty and saves every sheet. The iX500 pulls through any remaining
paper when the session ends, so Ferriscan intentionally provides no sheet-limit or early-stop option. Use
only a stack you intend to scan completely. With `--duplex` one sheet yields a front and a back image.
`--debug` prints the raw 32-byte image-info block per sheet.

Each page file is written through a temporary file and then renamed into place, so an interrupted run
cannot leave a truncated image. Existing `pageNNN_*.jpg` files are never replaced unless `--force` is
given; without it, the scan fails before writing.

## Development

Install a stable Rust toolchain with Rustfmt and Clippy. The repository's toolchain configuration selects
these components automatically when using rustup. Building binaries and tests also requires a system C
linker. On NixOS, if `cc` is not installed globally, prefix the build or test command with
`nix shell nixpkgs#stdenv.cc --command`.

```sh
cargo run
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The same checks run in GitHub Actions. Normal tests must not require a physical scanner or an MCP server.
The optional Rust and Slint MCP development integrations are documented in
[docs/development.md](docs/development.md).

## License

Ferriscan is available under the [MIT License](LICENSE). Adapted ScanSnack documentation retains its
original copyright and license notice; see [NOTICE.md](NOTICE.md).
