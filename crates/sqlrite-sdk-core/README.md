# sqlrite-sdk-core

`sqlrite-sdk-core` provides the shared request, response, and validation types used by SQLRite SDKs and service clients.

Use this crate when you need the stable envelope contracts without depending on the full SQLRite engine.

## What it includes

- query request types
- SQL request types
- query response envelopes
- validation errors
- query-profile helpers and defaults

## Typical use cases

- building SDKs on top of SQLRite
- writing thin Rust clients for SQLRite HTTP or gRPC adapters
- sharing request and response contracts across services

## Example

```rust
use sqlrite_sdk_core::{QueryRequest, DEFAULT_TOP_K};

let request = QueryRequest {
    query_text: Some("local memory".to_string()),
    top_k: Some(DEFAULT_TOP_K),
    ..QueryRequest::default()
};

request.validate()?;
# Ok::<(), sqlrite_sdk_core::ValidationError>(())
```

## Versioning

`sqlrite-sdk-core` tracks SQLRite releases and is intended to stay wire-compatible with the corresponding `sqlrite` release line.

- crate docs: <https://docs.rs/sqlrite-sdk-core>
- main project: <https://github.com/zavora-ai/SQLRite>
