# Packaging and Install Channels

This document covers Sprint 2 packaging and installer channels for SQLRite.

## Channels Implemented

1. Homebrew formula template generation
2. winget manifest generation
3. Linux package generation (`.deb` / `.rpm`) via `nfpm`
4. Release archive generation (`.tar.gz` + SHA256)
5. Docker image for server mode
6. Curl-friendly release installer script
7. Source-based global install and update scripts with smoke tests

## Scripts

### Source-based install/update

- `scripts/sqlrite-global-install.sh`
- `scripts/sqlrite-global-update.sh`

These build from local source and run smoke tests by default.

### Release installer (curl-friendly)

- `scripts/sqlrite-install.sh`

Downloads release artifacts from GitHub Releases and installs `sqlrite` to `~/.local/bin` by default.

Example:

```bash
bash scripts/sqlrite-install.sh --version 0.5.0
```

### Release archive creation

- `scripts/create-release-archive.sh`

Example:

```bash
bash scripts/create-release-archive.sh --version 0.5.0
```

### Linux package generation (`.deb` / `.rpm`)

- `scripts/package-linux.sh`
- `packaging/nfpm/nfpm.yaml`

Example:

```bash
bash scripts/package-linux.sh --version 0.5.0
```

If `nfpm` is missing, the script still produces tar archives and skips deb/rpm output.

### Homebrew formula generation

- Template: `packaging/homebrew/sqlrite.rb.template`
- Generator: `scripts/generate-homebrew-formula.sh`

Example:

```bash
bash scripts/generate-homebrew-formula.sh \
  --version 0.5.0 \
  --macos-arm64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-aarch64-apple-darwin.tar.gz \
  --macos-arm64-sha <sha> \
  --macos-amd64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-x86_64-apple-darwin.tar.gz \
  --macos-amd64-sha <sha> \
  --linux-arm64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-aarch64-unknown-linux-gnu.tar.gz \
  --linux-arm64-sha <sha> \
  --linux-amd64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-x86_64-unknown-linux-gnu.tar.gz \
  --linux-amd64-sha <sha>
```

### winget manifest generation

- Templates: `packaging/winget/templates/*`
- Generator: `scripts/generate-winget-manifests.sh`

Example:

```bash
bash scripts/generate-winget-manifests.sh \
  --version 0.5.0 \
  --windows-amd64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-x86_64-pc-windows-msvc.tar.gz \
  --windows-amd64-sha <sha> \
  --windows-arm64-url https://github.com/zavora-ai/SQLRite/releases/download/v0.5.0/sqlrite-v0.5.0-aarch64-pc-windows-msvc.tar.gz \
  --windows-arm64-sha <sha>
```

## CI Workflows

1. `installer-smoke.yml`
- Runs global install/update smoke checks on Linux/macOS/Windows.
- Runs installed-binary `sqlrite quickstart` gate checks and uploads per-OS quickstart reports.

2. `packaging-channels.yml`
- Builds release archives on Linux/macOS/Windows.
- Builds Linux deb/rpm packages.
- Builds Docker image and performs container smoke check.

3. `release-bootstrap.yml`
- Builds release `sqlrite` binary artifacts for Linux/macOS/Windows on tags/manual triggers.

## Docker

- Dockerfile: `Dockerfile`
- Default runtime command:

```bash
sqlrite serve --db /data/sqlrite.db --bind 0.0.0.0:8099
```

Build and run locally:

```bash
docker build -t sqlrite:local .
docker run --rm -p 8099:8099 -v "$PWD:/data" sqlrite:local
```
