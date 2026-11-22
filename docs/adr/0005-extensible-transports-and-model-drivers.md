---
status: accepted
date: 2026-11-16
---

# Extensible transports and model drivers

## Context and problem statement

Wi-Fi is Ferriscan's first implementation milestone, but USB may follow. The initial iX1500 and iX500
drivers will also not be the only supported models forever. If application code directly selects sockets,
model types, or addresses, each new transport or model would affect use cases and the GUI.

## Decision drivers

- Wi-Fi must be implemented first without making USB prohibitively expensive later.
- Application and GUI code should not depend on a transport or scanner model.
- The model list should grow through isolated drivers rather than scattered conditionals.
- USB and unexamined models must not inherit unverified Wi-Fi assumptions.

## Considered options

- Implement Wi-Fi directly and refactor when USB or another model is added.
- Provide a generic low-level byte transport immediately and assume all models and transports fit it.
- Define stable application-facing scanner and discovery ports, separate model drivers, and shared
  transport adapters, while deferring unknown low-level USB abstractions.

## Decision outcome

Extend the ports-and-adapters architecture from ADR 0002 with two independent extension axes:

1. **Transport:** Wi-Fi adapters come first. A future USB adapter implements the same high-level scanner
   and discovery ports. Shared transport code lives separately, such as `adapters/scansnap_wifi` and a
   future `adapters/scansnap_usb`.
2. **Model:** Each scanner model has its own driver module, protocol source of truth, and tests. Model
   drivers may use shared transports but never import another model driver.

Application-facing scanner and discovery traits remain independent of transport libraries and model
types. Discovery results must provide enough model, transport, and addressing information for the
composition root to select a driver. The composition root owns the driver registry and construction;
model-name conditionals do not belong in use cases or the GUI.

The first Wi-Fi implementation must establish these port boundaries. No empty USB driver or speculative
low-level USB trait is required now. USB discovery, framing, registration, and scanning require separate
evidence and must not be assumed to match Wi-Fi.

Adding a model requires:

- a `docs/protocol-<model>.md` source of truth with sources and open questions;
- a model-specific adapter and transport selection;
- anonymised byte-level and state-transition tests;
- registration in the composition root and an entry in `docs/models.md`;
- explicit support status based on implementation and test evidence.

## Consequences

- Wi-Fi and future USB drivers can be selected without changing application or GUI use cases.
- Additional models are isolated and visibly registered.
- Common transport behavior is reusable without assuming model compatibility.
- The high-level ports need enough capability information to avoid lowest-common-denominator scanning.
- Unknown USB behavior remains undocumented and unimplemented until evidence exists.
