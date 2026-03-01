# SQLRite TypeScript SDK

TypeScript SDK for SQLRite HTTP query surfaces.

## Install

```bash
npm install @sqlrite/sdk
```

## Usage

```ts
import { SqlRiteClient } from "@sqlrite/sdk";

const client = new SqlRiteClient("http://127.0.0.1:8099");
const openapi = await client.openapi();
const query = await client.query({ query_text: "agent memory", top_k: 2 });
const sql = await client.sql("SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;");

console.log(openapi, query, sql);
```
