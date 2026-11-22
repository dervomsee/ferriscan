# Ferriscan contributor and agent instructions

## Project purpose and scope

Ferriscan is an independent Rust application for using ScanSnap scanners without ScanSnap Home. Wi-Fi
is the first implementation milestone and primary transport. USB may follow as a separate adapter behind
the same application-facing ports. The initial protocol targets are the ScanSnap iX1500 and iX500, with
an architecture designed for additional model drivers. The desktop GUI will use Slint. Linux is the
primary platform; Windows and macOS are optional best-effort targets, not promised support. Documentation
is not an implementation claim: a model, transport, or platform is supported only after its
implementation and relevant tests are in place. USB, additional models, and eSCL/AirScan are therefore
not current support claims.

The repository is currently being bootstrapped. Keep user-facing status statements accurate and do not
claim live-device validation unless it actually happened.

## Language and communication

- Use English for every committed artifact: code, identifiers, comments, doc comments, tests, CLI and log
  messages, documentation, workflows, and commit messages.
- Keep device output and user interface quotations verbatim, even when they are not English.
- Keep responses to the user concise and in German unless the user asks otherwise.

## ScanSnack reference and attribution

A temporary `scansnack-main/` checkout may be used as a read-only reference and is intentionally ignored
by Git. Do not modify it or make the Rust build depend on it. The public upstream is
<https://github.com/elte0/scansnack>; the repository itself does not require a local checkout.

The protocol documents under `docs/` are adapted from ScanSnack under the MIT license. Preserve the
attribution and license notices in `NOTICE.md` and `LICENSE`. Use the documented protocol facts to write
idiomatic, independently structured Rust; do not mechanically translate the Python implementation.
Unless a document explicitly says otherwise, observations and live tests described in the inherited
protocol documents belong to ScanSnack, not Ferriscan.

## Architecture

Use a ports-and-adapters architecture with dependencies pointing inward:

```text
src/
  core/                    domain types, scan settings, pages, errors, image processing
  ports/                   transport-neutral scanner, discovery, and output traits
  adapters/
    scansnap_wifi/         shared Wi-Fi framing, discovery, notifications, and SCSI transport
    scansnap_usb/          future shared USB transport primitives, after protocol analysis
    ix1500/                iX1500 registration, profiles, and model-specific scanning
    ix500/                 iX500 registration and model-specific scanning
    <model>/               future model-specific drivers
    slint_gui/             Slint GUI adapter (when the GUI is introduced)
  app/                     use cases, driver registry, and composition root
  main.rs                  thin executable entry point
```

- `app` may depend on adapters, ports, and core; adapters depend on ports and core; ports depend only on
  core; core depends on none of the other layers.
- Model adapters must not import each other. They may use shared transport adapters such as
  `adapters/scansnap_wifi`; a shared transport must never depend on a model adapter.
- Application and GUI code use transport-neutral scanner and discovery ports, never concrete model
  drivers, sockets, or USB APIs. Transport selection and addressing details stay behind these ports.
- The composition root owns driver registration and selection using the discovered model and transport.
  Do not scatter model-name conditionals through application or GUI code.
- A future USB adapter implements the same high-level scanner and discovery ports as Wi-Fi. Do not assume
  that USB uses the Wi-Fi framing, registration, discovery, or state machine; document and test it
  independently before implementation.
- Core and ports must remain free of socket, USB-library, GUI, and async-runtime dependencies.
- Construct concrete adapters only in the composition root. Keep protocol I/O out of GUI code and GUI
  threads when a GUI is introduced.
- Add each model in its own adapter with its own protocol document and tests. Reuse an existing model or
  transport implementation only where evidence confirms compatibility.
- Do not create an empty USB implementation or speculate about its low-level protocol. The required
  transport-neutral port boundary is part of the first Wi-Fi design; concrete USB modules land with real
  functionality. Do not create other empty layers or abstractions speculatively.

## GUI and platforms

- Use Slint for the desktop GUI. Keep Slint components and rendering types in `adapters/slint_gui` and
  declarative UI resources in a dedicated `ui/` directory; neither core nor ports may depend on Slint.
- Never run scanner or network I/O on the Slint UI thread. Use application workers and deliver results
  back through Slint's event-loop mechanisms.
- Target Linux first. Keep core and protocol code portable, isolate unavoidable OS-specific code, and do
  not claim Windows or macOS support until it is built and tested there.
- Use Slint's headless testing backend for automated UI tests. MCP is a development aid, not an
  application requirement or a replacement for automated tests.
- Enable Slint MCP only deliberately during development. Keep it on its default loopback address and do
  not expose it through `SLINT_MCP_HOST=0.0.0.0` without an explicit user request. See
  `docs/development.md` for both MCP integrations.

## Protocol documentation and evidence

- `docs/protocol.md` is the overview. `docs/protocol-<model>.md` is the single source of truth for that
  model's packet layouts, sequences, state machine, sources, and open questions.
- Never invent protocol values or silently turn a hypothesis into a fact. Update the relevant protocol
  document with every new finding.
- Separate observations from assumptions. Mark unconfirmed code with `// UNVERIFIED:` and add a matching
  item to the model document's "Open questions" section.
- Prefer byte-level golden tests based on anonymised, documented observations. Tests must not require a
  physical scanner. Never alter captured evidence merely to make an implementation pass.
- When implementing an inherited detail, cite the relevant protocol section in a focused test or comment
  only where that adds useful context; avoid noisy documentation duplication.
- Discovery can be unreliable while a scanner holds a session. Tests and diagnostics should support a
  directly configured scanner IP rather than relying only on broadcast discovery.

## Privacy and safety

- Never commit raw captures, scanned documents, test logs, identity files, passwords, network identifiers,
  or personal data. Use the ignore rules and anonymised fixtures.
- The iX500 identity file stores the scanner Wi-Fi password in plain text with mode 0600; never print it,
  log it, or add it to the repository ([ADR 0006](docs/adr/0006-ix500-identity-storage.md)).
- Live tests must assume that the iX500 will pull through the entire stack. Use only paper intended for a
  complete scan and do not leave documents unattended. Ferriscan has no sheet-limit option: it reads and
  saves every sheet until the feeder is empty.
- Do not add manufacturer firmware, binaries, or proprietary source code.
- Keep `docs/disclaimer.md` in place and linked from the README.
- Registration and pairing can change scanner state. Destructive or state-changing live tests require an
  explicit user request and a clearly identified target device.

## Rust engineering rules

- Use the stable toolchain and settings in `rust-toolchain.toml`; unsafe code is forbidden unless a future
  ADR explicitly allows a narrowly justified exception.
- Handle errors deliberately. Avoid `unwrap` and `expect` in production paths; use them only in tests or
  genuinely infallible, locally evident cases.
- Keep modules small and test parsing, serialization, state transitions, and error handling without real
  hardware.
- Add dependencies only when they provide clear value. Prefer established, maintained crates and avoid
  adding a runtime before asynchronous I/O is actually needed.
- Preserve protocol byte order, integer widths, frame lengths, and exact magic values. Do not use native
  endianness for wire data.

## Decisions and changelog

- Record architectural and protocol decisions as MADR documents in `docs/adr/NNNN-title.md`.
- Never silently rewrite an accepted decision. Add a superseding ADR when a decision changes.
- Add user-, packager-, or protocol-reader-visible changes to `## [Unreleased]` in `CHANGELOG.md` in the
  same change. Pure refactoring, tests, and CI-only tweaks need no entry.

## Required checks

Run all applicable checks before finishing:

```sh
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

A task is done only when the checks pass, documentation matches behavior, and any required changelog entry
is present. If a check cannot run, report the exact reason instead of claiming success.
