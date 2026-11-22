---
status: accepted
date: 2026-11-16
---

# Slint for the desktop GUI

## Context and problem statement

Ferriscan needs a desktop GUI that fits a Rust codebase, can isolate scanner I/O from interface code, and
can later run headless UI tests. The GUI toolkit should support Linux first without preventing optional
Windows and macOS ports.

## Decision drivers

- Declarative, maintainable UI definitions.
- Rust integration and a clear separation from domain and protocol code.
- Headless testing and agent-assisted UI inspection during development.
- Cross-platform potential without promising every platform immediately.

## Considered options

- Slint.
- Immediate-mode Rust GUI toolkits.
- Native or externally bound GUI frameworks.

## Decision outcome

Use Slint for the desktop GUI. Keep generated component access, Slint types, and event-loop integration in
the `adapters/slint_gui` adapter, with declarative resources under `ui/`. Core, ports, and protocol
adapters must not depend on Slint. Scanner I/O runs outside the UI thread.

Use Slint's testing backend for headless automated tests. Its embedded MCP server is an optional
loopback-only development aid enabled with `slint/mcp`; it is not part of normal application operation.

The GUI is not implemented yet. When adding the dependency, select a Slint version compatible with the
project's Rust version and review the applicable Slint license option. Slint 1.19 provides the documented
`mcp` feature and requires Rust 1.95; this ADR does not itself pin that version or change the MSRV.

## Consequences

- UI definitions and Rust application behavior have a clear toolkit boundary.
- UI behavior can be tested headlessly and inspected through MCP during development.
- Slint version, renderer, backend, licensing, and system dependencies must be considered when the GUI is
  introduced.
- Optional platforms still require their own builds and tests.
