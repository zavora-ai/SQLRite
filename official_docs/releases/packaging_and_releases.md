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

What the default container does:

| Behavior | Result |
|---|---|
| mounts `/data` | persisted database files live outside the container |
| starts `sqlrite serve --db /data/sqlrite.db --bind 0.0.0.0:8099` | SQLRite serves on port `8099` |
| finds no existing database | SQLRite creates an empty database with schema applied |

If you want query results immediately, seed the mounted database first:

```bash
mkdir -p docker-data
sqlrite init --db ./docker-data/sqlrite.db --seed-demo
docker run --rm -p 8099:8099 -v "$PWD/docker-data:/data" sqlrite:local
```

## 4. Start a Seeded Demo Server with Docker Compose

Use the compose example when you want a one-command local server with demo data already loaded.

```bash
docker compose -f deploy/docker-compose.seeded-demo.yml up --build
```

What this compose file does:

| Step | Result |
|---|---|
| runs `sqlrite-init` once | creates `/data/sqlrite.db` and seeds demo content if the volume is empty |
| starts `sqlrite` | serves the seeded database on port `8099` |
| reuses a named volume | keeps data across restarts |

Verify it:

```bash
curl -fsS http://127.0.0.1:8099/readyz
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"query_text":"agent memory","top_k":3}' \
  http://127.0.0.1:8099/v1/query
```

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
