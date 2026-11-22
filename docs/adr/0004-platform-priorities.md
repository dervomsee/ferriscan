---
status: accepted
date: 2026-11-16
---

# Linux first with optional Windows and macOS targets

## Context and problem statement

Ferriscan initially needs one clearly defined desktop environment for development, testing, and releases.
Cross-platform support is desirable, but promising several platforms before they are exercised would make
support expectations and testing requirements unclear.

## Decision drivers

- Linux is the primary development and deployment environment.
- Networking, paths, configuration storage, and GUI backends can differ between operating systems.
- Optional ports should not force unnecessary Linux-specific design decisions.

## Considered options

- Linux only.
- Equal support for Linux, Windows, and macOS from the start.
- Linux first, with Windows and macOS as optional best-effort targets.

## Decision outcome

Prioritize Linux for implementation, required CI, testing, and initial releases. Windows and macOS are
optional targets that may be added when their builds and tests are maintained. Do not describe either as
supported merely because Rust or Slint is cross-platform.

Keep domain and protocol code portable. Isolate operating-system integration behind appropriate adapters
and use conditional compilation only at that boundary. Avoid hard-coded user paths and platform-specific
shell commands in application code.

## Consequences

- Linux correctness takes precedence over speculative portability work.
- New code must not introduce avoidable platform lock-in.
- Windows and macOS support claims require explicit implementation and test evidence.
