---
status: accepted
date: 2026-11-16
---

# Local iX500 identity and password storage

## Context and problem statement

The iX500 validates the scanner Wi-Fi password on every registration (control command 0x11): a registration
without a password is rejected with status −2 and a wrong password with −3. Ferriscan registers before each
scan session, so without persistence the user would have to supply the password on every invocation.

The iX500 also accepts only host IDs from a table that ScanSnap Home writes over USB, so the client must
reuse a host ID the scanner already knows.

## Decision drivers

- Scanning should not require retyping the Wi-Fi password for every run.
- The scanner password must never be printed, logged, or committed.
- A wrong or stale host ID must produce a clear error rather than a silently broken session.
- The identity must be inspectable and editable by the user.

## Considered options

- Keep the password only in memory and require an option or environment variable for every run.
- Store the identity and password in a configuration file with owner-only permissions.
- Use a system keyring or secret service.

## Decision outcome

Use a plain-text identity file at `$XDG_CONFIG_HOME/ferriscan/ix500.conf`, falling back to
`~/.config/ferriscan/ix500.conf`. It stores the host ID, the interface MAC, the notification port, and the
scanner password. The file and its directory are created with owner-only permissions (0600/0700), and it is
written atomically.

Credential precedence is: explicit `--password`, then `FERRISCAN_IX500_PASSWORD`, then the stored password,
then the factory default derived from the last four serial-number digits. An explicit empty password means
that the scanner has no password and must not fall back to the default.

A stored host ID is preferred; otherwise the client adopts the host ID the scanner reports as paired and
reports a clear error when the scanner reports none.

`--identity PATH` selects another file and `--no-save` disables writing it. A failed save is a warning, not a
scan failure.

## Consequences

- The password is available in plain text to anything that can read the user's configuration directory.
  This is the same trade-off ScanSnap Home and similar tools make, and it is documented to the user.
- Users can inspect and edit the file; unknown keys and malformed values are rejected instead of guessed.
- The password never appears in command output. Passing it as a command-line argument can still expose it in
  the process list, so the environment variable or the file is preferred.
- A future secret-service integration can replace the file without changing the adapter interface.
