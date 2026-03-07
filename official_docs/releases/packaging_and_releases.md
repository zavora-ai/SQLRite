# Packaging and Releases Guide

This guide covers the developer-facing packaging and release commands that ship with the repository.

## Packaging Options

| Artifact | Command | Use when |
|---|---|---|
| release archive | `scripts/create-release-archive.sh` | local release packaging |
| Linux packages | `scripts/package-linux.sh` | `.deb` or `.rpm` packaging |
| Docker image | `docker build` | containerized deployment |

## 1. Create a Release Archive

```bash
bash scripts/create-release-archive.sh --version 1.0.0
```

Expected result:

| Output | Meaning |
|---|---|
| `dist/sqlrite-v1.0.0-<target>.tar.gz` | versioned release archive |
| `dist/sqlrite-v1.0.0-<target>.sha256` | checksum for the archive |

## 2. Build Linux Packages

When `nfpm` is installed:

```bash
bash scripts/package-linux.sh --version 1.0.0
```

Use this when you want Linux-native package artifacts rather than a tarball.

## 3. Build and Run the Docker Image

```bash
docker build -t sqlrite:local .
docker run --rm -p 8099:8099 -v "$PWD:/data" sqlrite:local
```

Use this when you want a containerized local smoke test or deployment baseline.

## Release Checklist

| Step | Why it matters |
|---|---|
| build archive | validates distributable packaging |
| verify checksum | proves artifact integrity |
| run a smoke test | proves the package is usable |
| publish release notes | gives developers a canonical release summary |

## Deeper References

- `project_docs/releases/v1.0.0.md`
- `project_docs/release_policy.md`
- `project_docs/runbooks/ga_release_train.md`
