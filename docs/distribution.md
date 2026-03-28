# Distribution

This guide covers the packaging and shipping paths that are current today.

## Packaging options

| Artifact | Command | Notes |
|---|---|---|
| source install | `cargo install --path .` | installs full CLI toolchain |
| release archive | `bash scripts/create-release-archive.sh --version 1.0.0` | packages `sqlrite` |
| release installer | `bash scripts/sqlrite-install.sh --version 1.0.0` | installs `sqlrite` |
| Docker image | `docker build -t sqlrite:local .` | service deployment baseline |
| seeded Docker Compose demo | `docker compose -f deploy/docker-compose.seeded-demo.yml up --build` | seeded local demo server |

## Release archive

```bash
bash scripts/create-release-archive.sh --version 1.0.0
```

Outputs:

- `dist/sqlrite-v1.0.0-<target>.tar.gz`
- `dist/sqlrite-v1.0.0-<target>.sha256`

## Linux packages

When `nfpm` is installed:

```bash
bash scripts/package-linux.sh --version 1.0.0
```

## Docker

```bash
docker build -t sqlrite:local .
docker run --rm -p 8099:8099 -v "$PWD/docker-data:/data" sqlrite:local
```

Container behavior:

| Behavior | Result |
|---|---|
| database path | `/data/sqlrite.db` |
| empty mounted directory | SQLRite creates an empty schema-applied database |
| default command | `sqlrite serve --db /data/sqlrite.db --bind 0.0.0.0:8099` |

If you want a ready-to-query container immediately:

```bash
mkdir -p docker-data
sqlrite init --db ./docker-data/sqlrite.db --seed-demo
docker run --rm -p 8099:8099 -v "$PWD/docker-data:/data" sqlrite:local
```

## Seeded Docker Compose demo

```bash
docker compose -f deploy/docker-compose.seeded-demo.yml up --build
```

This flow:

- seeds the database once if the volume is empty
- starts the HTTP server on `8099`
- keeps the database in a named volume
