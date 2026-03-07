# SQLRite Official Documentation

This is the primary product documentation for SQLRite.

It is written for developers who want to install SQLRite, understand how retrieval works, integrate it into an application, and run it in production without having to read the entire repository.

## Documentation Surfaces

| Surface | Purpose | Start here |
|---|---|---|
| `official_docs/` | Primary product guide | `official_docs/README.md` |
| `example_docs/` | Example-by-example walkthroughs | `example_docs/README.md` |
| `project_docs/` | Mirror of the current legacy docs tree | `project_docs/README.md` |
| `docs/` | Compatibility layer still referenced by scripts and release tooling | `docs/README.md` |

## Reading Paths

### I want to get started quickly

1. `official_docs/getting_started/installation.md`
2. `official_docs/getting_started/quickstart.md`
3. `official_docs/querying/query_patterns.md`
4. `official_docs/sql/sql_retrieval_guide.md`

### I want to integrate SQLRite into an app or service

1. `official_docs/getting_started/quickstart.md`
2. `official_docs/integrations/server_and_api_guide.md`
3. `official_docs/integrations/sdk_guide.md`
4. `example_docs/README.md`

### I want to run SQLRite operationally

1. `official_docs/security/security_and_multi_tenant.md`
2. `official_docs/operations/operations_and_benchmarks.md`
3. `official_docs/releases/packaging_and_releases.md`

### I want to migrate existing data

1. `official_docs/migrations/migration_guide.md`
2. `project_docs/migrations/sqlite_to_sqlrite.md`
3. `project_docs/migrations/pgvector_to_sqlrite.md`
4. `project_docs/migrations/api_first_vector_db_patterns.md`

## Topic Index

| Topic | Guide |
|---|---|
| Install SQLRite | `official_docs/getting_started/installation.md` |
| Run the first query | `official_docs/getting_started/quickstart.md` |
| Learn the CLI retrieval patterns | `official_docs/querying/query_patterns.md` |
| Use SQL-native retrieval | `official_docs/sql/sql_retrieval_guide.md` |
| Ingest and reindex data | `official_docs/ingestion/ingestion_and_reindexing.md` |
| Migrate from SQLite, libSQL, pgvector, and API-first vector exports | `official_docs/migrations/migration_guide.md` |
| Configure RBAC, audit, and tenant keys | `official_docs/security/security_and_multi_tenant.md` |
| Use HTTP, gRPC, and MCP | `official_docs/integrations/server_and_api_guide.md` |
| Use the Python and TypeScript SDKs | `official_docs/integrations/sdk_guide.md` |
| Run health, backup, compaction, and benchmarks | `official_docs/operations/operations_and_benchmarks.md` |
| Build release artifacts | `official_docs/releases/packaging_and_releases.md` |

## What These Guides Optimize For

- commands that work as written on a developer machine
- realistic expected results instead of abstract descriptions
- tables when they improve scanability
- clear separation between public product guidance and internal project history

## When to Drop into `project_docs/`

Use `project_docs/` when you need the deeper reference material that still lives in the legacy docs tree, especially for:

- RFCs and design rationale
- release policy and release notes
- migration runbooks
- HA and replication references
- security posture and threat model details
