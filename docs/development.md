# Development tools

## Platforms and GUI

Linux is the primary platform. Windows and macOS are optional best-effort targets and are not currently
supported or tested. The desktop GUI will use Slint, but the current executable is still a skeleton and
has no Slint dependency or user interface.

## MCP overview

The project-local [Pi MCP configuration](../.pi/mcp.json) defines two optional development servers:

| Server | Transport | Initial state | Purpose |
|---|---|---|---|
| `rust` | stdio | enabled after project trust | Cargo build, test, lint, dependency, and toolchain tools |
| `slint` | streamable HTTP at `http://127.0.0.1:8080/mcp` | disabled | Inspect and drive a running Slint GUI |

Pi reads `.pi/mcp.json` only after the project is trusted. After changing the configuration inside an
interactive Pi session, run `/reload`. Neither MCP server is needed for normal builds, tests, or releases.

## Rust MCP server

Install the server once with rustup and Cargo:

```sh
cargo install rust-mcp-server --locked
```

The project configuration starts `~/.cargo/bin/rust-mcp-server --workspace .` over stdio. The explicit
path avoids relying on Cargo's binary directory being in `PATH`. To recreate the entry with Pi:

```sh
pi mcp add -l rust --exposure deferred \
  --description 'Rust build, test, lint, dependency, and toolchain tools for Ferriscan' \
  -- '~/.cargo/bin/rust-mcp-server' --workspace .
```

The server can run commands that change code, dependencies, toolchains, and build artifacts. Review tool
calls with the same care as equivalent shell commands. Project trust is intentionally required before Pi
loads this configuration.

## Slint testing and MCP

The [Slint testing backend documentation](https://docs.slint.dev/latest/docs/rust/i_slint_backend_testing/)
provides headless UI testing and an embedded MCP server. Applications use the `slint` crate and its
`slint/mcp` feature; they do not use the internal `i-slint-backend-testing` crate directly.

Slint 1.19 documents the `mcp` feature and requires Rust 1.95. The current project has no Slint dependency
and retains its earlier MSRV until GUI implementation selects and records a compatible version.

Once a real Slint GUI exists, start it for headless agent inspection with:

```sh
SLINT_EMIT_DEBUG_INFO=1 SLINT_MCP_PORT=8080 SLINT_BACKEND=headless \
    cargo run -p ferriscan --features slint/mcp
```

This command is a template for the future GUI and does not run against the current skeleton.

- `SLINT_EMIT_DEBUG_INFO=1` embeds element metadata required for introspection.
- `SLINT_MCP_PORT=8080` opts into the embedded server on port 8080. Without this variable the server does
  not start.
- `SLINT_BACKEND=headless` selects a windowless software-rendered backend for CI, containers, or agent
  environments without a display. This MCP-oriented backend setting is unstable and must not be used as
  production application logic.
- The server binds to `127.0.0.1` by default. Keep it local; do not set `SLINT_MCP_HOST=0.0.0.0` unless
  remote access has explicitly been requested and its risks accepted.

The `slint` entry in `.pi/mcp.json` points to `http://127.0.0.1:8080/mcp` and is disabled by default.
Enable it through Pi's `/mcp` interface only while the application server is running, then inspect its
instructions and tools. Disable it again when it is not in use. Do not add a separate `mcp` entry to the
application's `[features]` section; pass `slint/mcp` on the command line as shown above.

Headless automated UI tests should use Slint's testing APIs directly and must not depend on an agent or a
running MCP session.
