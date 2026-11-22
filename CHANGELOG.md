# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed

- Removed the misleading `--max-sheets` option. Every scan now always reads and saves every sheet until the
  feeder is empty, matching the iX500's behavior of clearing the stack when the session ends.

### Changed

- `ix500 scan` reports sheet and image counts from the adapter's scan result instead of counting in the
  command layer.

### Fixed

- Resolve the Linux interface MAC through sysfs when the matching `getifaddrs` IPv4 entry has no
  link-layer address, fixing iX500 discovery on typical WLAN interfaces.
- Make targetless iX500 discovery work when scanners ignore broadcast: probe directed and limited
  broadcast with the correct socket options, then collect concurrent unicast probes across the local
  subnet, bounded to a /24 per interface.
- Read the iX500 image-info resolution fields at their live-validated offsets, so the reported dpi matches
  the requested resolution instead of showing zero for the x axis.
- Delay unicast discovery retries slightly, matching how ScanSnap Home spaces its probes.
- Fall back to broadcast and local-subnet probing when a directly addressed scanner does not answer a
  unicast probe, so `scan`, `info`, and `register` survive the silence that follows a session.

### Added

- Transport-neutral scan result and output boundary: `ScannerPort::scan` now returns a `ScanSummary`
  (sheets and images), and a `ScanOutput` port persists sheets with a filesystem adapter that writes page
  files through a temporary file and refuses to overwrite existing files unless `ix500 scan --force` is
  given.
- `ix500 scan --force` to replace existing page files, and distinct `unsupported setting` and
  `refusing to overwrite existing file` error messages.
- Persistent iX500 identity file storing the paired host ID and the scanner Wi-Fi password (mode 0600), so
  scanning no longer requires the password on every run. `--identity` and `--no-save` control it, and
  `ix500 info` now registers the host because the scanner requires that before its name query.
- `ix500 scan --debug` to print the raw image-info block per sheet.
- Live-validated iX500 discovery, registration, and color scanning on one device.
- Initial Rust executable skeleton, stable toolchain configuration, and continuous-integration checks.
- Initial ScanSnap iX500 Wi-Fi vertical slice: transport-neutral ports, UDP discovery, bounded `VENS` TCP
  framing, local interface MAC selection on Linux, device name/status queries, and 0x11 registration with
  documented password encoding and error statuses.
- Mock-based UDP and TCP integration tests for the iX500 protocol; no physical scanner is required.
- iX500 SCSI tunnel and scan sequence with captured E9/D4 parameter blocks, color JPEG reads, duplex or
  simplex output, page-size and multi-feed options, feeder-empty/paper-jam errors, and D6 session release.
- `ferriscan ix500 scan` writes raw front/back JPEG files; rotation, cropping, deskewing, and grayscale
  conversion remain future work.
- Decisions to use Slint for the desktop GUI, prioritize Linux, and treat Windows and macOS as optional
  best-effort targets.
- Optional project-local Rust and Slint MCP development-server configuration and documentation.
- Extensible transport and model-driver architecture: Wi-Fi first, a future USB adapter behind common
  scanner ports, and a documented process for adding scanner models.
- Contributor and coding-agent instructions covering English repository language, architecture, testing,
  privacy, and evidence-based protocol work.
- ScanSnack-derived protocol documentation for the ScanSnap iX1500 and iX500, with attribution and a
  reverse-engineering disclaimer.
