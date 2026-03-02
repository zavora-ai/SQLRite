#!/usr/bin/env node

import { SqlRiteClient } from "../../sdk/typescript/dist/index.js";

function parseArgs(argv) {
  const out = {
    baseUrl: "http://127.0.0.1:8099",
    query: "agent memory",
    topK: 2
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--base-url") {
      i += 1;
      out.baseUrl = argv[i];
    } else if (arg === "--query") {
      i += 1;
      out.query = argv[i];
    } else if (arg === "--top-k") {
      i += 1;
      out.topK = Number.parseInt(argv[i], 10);
    } else if (arg === "--help" || arg === "-h") {
      console.log(
        "usage: node examples/agent_integrations/typescript_memory_agent.mjs [--base-url URL] [--query TEXT] [--top-k N]"
      );
      process.exit(0);
    }
  }

  return out;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const client = new SqlRiteClient(args.baseUrl);
  const payload = await client.query({ query_text: args.query, top_k: args.topK });
  console.log(JSON.stringify(payload, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
