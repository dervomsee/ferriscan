---
status: accepted
date: 2026-11-16
---

# Ports-and-adapters architecture

## Context and problem statement

Ferriscan will combine uncertain model-specific network protocols with scan-domain logic and potentially
several user interfaces. Direct dependencies from domain or UI code to sockets would make tests require
hardware and would spread protocol changes throughout the application.

## Decision drivers

- Protocol knowledge should remain isolated and testable without a scanner.
- The iX1500 and iX500 share transport primitives but have different registration and scanning details.
- Domain logic must remain independent of networking, GUI frameworks, and async runtimes.

## Considered options

- A flat module layout governed only by conventions.
- Ports and adapters with explicit dependency direction.

## Decision outcome

Use ports and adapters with dependencies pointing inward: `app` → `adapters` → `ports` → `core`.
Model adapters do not depend on each other. Shared framing, discovery, notification, and SCSI code lives
in `adapters/scansnap_wifi`, which has no dependency on a model adapter. Concrete adapters are constructed
only by the composition root.

Modules are introduced as functionality requires them rather than as empty placeholders.

## Consequences

- Protocol adapters can be tested with byte fixtures and fake transports.
- Domain and application logic remain independent of device networking.
- Shared transport behavior is implemented once without merging model-specific dialects.
- The structure adds modules and trait indirection, which must remain justified by real behavior.
