# Scanner models and transport status

Ferriscan's first implementation milestone is Wi-Fi scanning. USB may follow as a separate transport
adapter behind the same application-facing scanner and discovery interfaces. The driver architecture is
designed to add further scanner models over time.

A model or transport listed as planned is not supported yet. Support requires a Rust implementation and
relevant tests; live-device validation is reported separately and never inherited from ScanSnack.

| Model | Wi-Fi | USB | Protocol source |
|---|---|---|---|
| ScanSnap iX1500 | Initial target; implementation pending | Optional future work; not analysed | [iX1500 Wi-Fi protocol](protocol-ix1500.md) |
| ScanSnap iX500 | Discovery, registration, and raw color JPEG scanning live-validated on one device and covered by mock tests | Optional future work; not analysed | [iX500 Wi-Fi protocol](protocol-ix500.md) |
| Other ScanSnap models | Not supported; no protocol examined | Not supported; no protocol examined | Required before implementation |

The existing protocol write-ups document ScanSnack's Wi-Fi findings. They do not establish USB behavior
or compatibility with other ScanSnap models.

## Adding a scanner model

Each new model must remain isolated from other model drivers and from application or GUI conditionals:

1. Document observed protocol behavior in `docs/protocol-<model>.md`, including sources, transport,
   packet layouts, state transitions, and open questions.
2. Add a model-specific adapter. Reuse shared Wi-Fi or future USB primitives only where evidence confirms
   compatibility; never copy another model's values on the assumption that they match.
3. Implement the transport-neutral scanner and discovery ports. Keep model and transport selection in the
   composition root's driver registry.
4. Add anonymised golden tests for wire formats and state transitions. Normal tests must not require the
   physical device.
5. Update this table and the changelog. Change the status to supported only when the implementation and
   tests actually provide it.

## Adding USB

USB work starts only after the initial Wi-Fi milestone and requires its own protocol evidence. A future
USB adapter must implement the same high-level scanner and discovery ports as Wi-Fi, but it may have
completely different discovery, framing, registration, and scanning behavior internally. No USB support
is claimed by the presence of the architectural extension point.
