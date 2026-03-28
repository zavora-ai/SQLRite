# Release Policy

This file describes the public release expectations for SQLRite.

## Versioning

SQLRite uses semantic versioning.

| Release type | Meaning |
|---|---|
| patch | bug fixes and packaging fixes |
| minor | backward-compatible features and performance improvements |
| major | breaking changes to public behavior or compatibility |

## Stability expectations for 1.x

The `1.x` line aims to keep these surfaces stable unless a major version is cut:

- core CLI command names
- SQL retrieval syntax such as `SEARCH(...)`
- compact HTTP response contract
- gRPC query service shape
- SDK-level query envelope structure

## Platform targets

Public builds target:

- Linux `x86_64` and `arm64`
- macOS `x86_64` and `arm64`
- Windows `x86_64` and `arm64`

## Distribution channels

| Channel | Current status |
|---|---|
| Cargo source install | primary full install path |
| GitHub release archive | supported |
| Docker image | supported |
| Linux packages via `nfpm` | optional build path |

## API compatibility artifact

The frozen API contract manifest lives at:

- `/Users/jameskaranja/Developer/projects/SQLRight/docs/contracts/api_freeze_v1.json`
