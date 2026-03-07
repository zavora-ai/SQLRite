# Example: `secure_tenant`

Source file:

- `examples/secure_tenant.rs`

## Purpose

This is the smallest secure multi-tenant embedding example in the repository.

Use it when you want to see:

- tenant-scoped ingest
- encrypted metadata fields
- tenant-specific key registration
- audit logging
- tenant-scoped search

## Run It

```bash
cargo run --example secure_tenant
```

## What the Example Does

| Step | Description |
|---|---|
| open database | creates an in-memory SQLRite database |
| create audit logger | configures JSONL audit logging with sensitive-field redaction |
| create secure wrapper | wraps SQLRite with access control and audit logging |
| register tenant key | assigns an active key for tenant `acme` |
| ingest encrypted metadata | inserts a chunk with encrypted `secret_payload` |
| run tenant query | searches as tenant `acme` |

## Observed Output

```text
== secure_tenant results ==
secure results: 1
top chunk: chunk-sec-1
```

## What to Notice

- the example uses `SecureSqlRite` rather than plain `SqlRite`
- the access context carries actor and tenant information
- the encrypted metadata flow is embedded directly in the Rust API, not bolted on externally

## Good Follow-Up Changes

- replace `AllowAllPolicy` with your own policy implementation
- persist the audit log and database paths
- add more tenants and verify isolation behavior
