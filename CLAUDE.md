# Ferriscan — context for coding agents

Read and follow [AGENTS.md](AGENTS.md); it is the binding contributor guide. This file records the
project context inherited from ScanSnack without duplicating all contributor rules.

## Current state

Ferriscan is a new Rust implementation, not a port with feature parity. The immediate goal is a Wi-Fi
scanner client for the ScanSnap iX1500 and iX500. The iX500 implementation covers discovery, Wi-Fi
framing, registration, and the SCSI scan sequence, with mock-based tests and live validation on one
device. Images are saved as raw scanner JPEGs; post-processing and grayscale conversion remain pending.
Scanner support must not be inferred from protocol documents alone. The planned desktop GUI uses Slint.
Linux is the primary platform; Windows and macOS are optional best-effort targets rather than supported
platforms at this stage.

## Architecture

Use ports and adapters: `app` → `adapters` → `ports` → `core`. Domain and port modules must not depend on
networking, USB libraries, GUI frameworks, or a concrete async runtime. Model-specific adapters are
independent and may share model-neutral transport primitives. Wi-Fi is implemented first; a future USB
transport must implement the same application-facing scanner and discovery ports without assuming the
Wi-Fi wire protocol. The composition root owns model/transport driver registration and selection.
Additional scanner models receive separate adapters, protocol documents, and tests. Slint belongs only
in the GUI adapter, and scanner I/O must never run on the UI thread.

See [ADR 0002](docs/adr/0002-ports-and-adapters-architecture.md) and
[ADR 0005](docs/adr/0005-extensible-transports-and-model-drivers.md). The
[model status list](docs/models.md) distinguishes planned drivers from actual support.

## Reverse engineering and protocol

- [Protocol overview](docs/protocol.md) links to the model-specific sources of truth.
- The iX1500 and iX500 documents were adapted from ScanSnack. Their captures, live tests, and open
  questions describe ScanSnack's evidence unless explicitly stated otherwise.
- Never invent protocol details. Mark uncertainty as `UNVERIFIED` in documentation and
  `// UNVERIFIED:` in Rust code.
- Add anonymised golden tests for wire formats and replayable sequences; never require a physical device
  in the normal test suite.
- Never commit raw captures, scanned content, private test logs, identities, or credentials. The iX500
  identity file stores the scanner Wi-Fi password in plain text; keep it at mode 0600 and out of Git
  ([ADR 0006](docs/adr/0006-ix500-identity-storage.md)).
- Live tests must assume that the iX500 will pull through the entire stack. Use only paper intended for a
  complete scan and do not suggest a sheet limit: Ferriscan always reads and saves every sheet until the
  feeder is empty.

## Development tools

The project configures an optional Rust MCP server and a disabled Slint MCP endpoint in `.pi/mcp.json`.
Slint MCP becomes useful only after the GUI and Slint's `mcp` feature exist. Neither server is required
for normal builds or tests. See [development tools](docs/development.md) for startup and trust details.

## Checks

```sh
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

## Conventions

All repository content and commit messages are English; quotations of device output remain verbatim.
Replies to the user are short and in German. Architectural decisions use MADR records under `docs/adr/`.
User-visible and protocol-documentation changes require an entry under `Unreleased` in `CHANGELOG.md`.
