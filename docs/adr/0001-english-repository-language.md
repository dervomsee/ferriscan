---
status: accepted
date: 2026-11-16
---

# English as the repository language

## Context and problem statement

Ferriscan is an open-source Rust project whose protocol documentation, tests, and code need to be usable
by contributors regardless of their native language. The project originates in a German-speaking context,
but mixing languages would make protocol findings and implementation details harder to follow.

## Decision drivers

- Protocol documentation should link naturally to readable code and tests.
- Contributors should not have to choose a language for each artifact.
- Device observations must remain faithful to their source.

## Considered options

- German for implementation and comments, English for public documentation.
- English for all committed artifacts.

## Decision outcome

Use English for code, comments, doc comments, identifiers, tests, CLI and log messages, documentation,
workflows, and commit messages. Quotations of device output and user interfaces remain verbatim, even in
another language. Conversations with the user may remain German and are not repository artifacts.

## Consequences

- Protocol readers can follow implementation and test evidence without a language switch.
- Existing device quotations are not translated or normalised.
