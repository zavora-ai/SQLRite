# Installation Guide

This guide covers the supported installation paths for SQLRite and shows how to verify that the install actually worked.

## At a Glance

| Option | Best for | Platforms |
|---|---|---|
| Install from source with Cargo | most developers, local builds, fast iteration | macOS, Linux, Windows |
| Install from this repo with helper scripts | repo-local convenience on Unix-like systems | macOS, Linux |
| Install from a GitHub release | fixed published artifact | release-dependent |

## Option 1: Install from Source with Cargo

This is the recommended path for most developers.

### Prerequisites

You need Rust and Cargo.

If they are not installed yet, get them from [rustup.rs](https://rustup.rs).

macOS / Linux:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows:

- download and run `rustup-init.exe` from [https://rustup.rs](https://rustup.rs)

Verify the toolchain:

```bash
rustc --version
cargo --version
```

What to expect:

| Command | Expected result |
|---|---|
| `rustc --version` | prints the Rust compiler version |
| `cargo --version` | prints the Cargo version |

### Step 1: Clone the Repository

```bash
git clone https://github.com/zavora-ai/SQLRite.git
cd SQLRite
```

What this does:

- downloads the SQLRite source code
- moves you into the repository root

### Step 2: Build and Install the Binaries

```bash
cargo install --path .
```

What this installs:

| Binary | Use for |
|---|---|
| `sqlrite` | main CLI |
| `sqlrite-security` | RBAC, audit export, key management |
| `sqlrite-reindex` | embedding refresh and model migration |
| `sqlrite-ingest` | resumable ingestion worker |
| `sqlrite-grpc-client` | gRPC smoke tests and client workflows |
| `sqlrite-bench-suite` | benchmark suite runner |
| `sqlrite-eval` | evaluation metrics runner |
| `sqlrite-mcp` | MCP tool server binary |
| `sqlrite-serve` | dedicated HTTP server binary |

Note:

- first install may take a minute because Cargo compiles dependencies locally

### Step 3: Confirm the Installed Binary Is the One Your Shell Uses

```bash
command -v sqlrite
sqlrite --help
```

What to expect:

| Check | Expected result |
|---|---|
| `command -v sqlrite` | points to the installed binary, typically under `~/.cargo/bin` |
| `sqlrite --help` | prints CLI usage and subcommands |

If `sqlrite` is not found, add Cargo's bin directory to your `PATH`.

macOS / Linux:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Windows:

Add `%USERPROFILE%\.cargo\bin` to your user `Path` environment variable, then restart the terminal.

### Step 4: Verify the Install End to End

Run these commands exactly:

```bash
sqlrite --help
sqlrite init --db sqlrite_verify.db --seed-demo
sqlrite query --db sqlrite_verify.db --text "local memory" --top-k 1
```

A successful install looks like this:

| Command | Expected result |
|---|---|
| `sqlrite --help` | CLI usage is printed |
| `sqlrite init ...` | a database file is created and seeded |
| `sqlrite query ...` | at least one retrieval result is returned |

Example query output:

```text
query_profile=balanced resolved_candidate_limit=500
results=1
1. demo-1 | doc=doc-a | hybrid=1.000 | vector=0.000 | text=1.000
   Rust and SQLite are ideal for local-first AI agents.
```

## Option 2: Install from This Repo with Helper Scripts

Use this when you are already in a local checkout and want the repo-provided global-install flow.

macOS / Linux:

```bash
bash scripts/sqlrite-global-install.sh
```

If your shell still cannot find `sqlrite`, add the default user bin directory:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Option 3: Install from a GitHub Release

Use this when you want a published archive rather than a local build.

```bash
bash scripts/sqlrite-install.sh --version 1.0.0
```

## Common Problems

### `cargo: command not found`

Rust is not installed or not on your `PATH`.

### `sqlrite: command not found`

The binary was installed, but its directory is not on your `PATH`.

### Build errors during install

Update the Rust toolchain:

```bash
rustup update
```

## Next Step

Once install works, continue with `official_docs/getting_started/quickstart.md`.
