import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { after, before, test } from "node:test";

import { SqlRiteApiError, SqlRiteClient } from "../dist/index.js";

const THIS_FILE = fileURLToPath(import.meta.url);
const TS_ROOT = path.resolve(path.dirname(THIS_FILE), "..");
const REPO_ROOT = path.resolve(TS_ROOT, "..", "..");

let tempDir;
let dbPath;
let baseUrl;
let serverProc;
let client;

function pickFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close(() => {});
        reject(new Error("failed to resolve ephemeral port"));
        return;
      }
      server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(address.port);
      });
    });
    server.on("error", reject);
  });
}

async function waitReady(url, timeoutMs = 8000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(`${url}/readyz`);
      if (response.ok) {
        return;
      }
    } catch {
      // keep retrying until timeout
    }
    await sleep(100);
  }
  throw new Error(`timed out waiting for ${url}/readyz`);
}

before(async () => {
  tempDir = mkdtempSync(path.join(os.tmpdir(), "sqlrite-ts-sdk-"));
  dbPath = path.join(tempDir, "integration.db");

  const sqlriteBin = path.join(REPO_ROOT, "target", "debug", "sqlrite");

  const build = spawnSync("cargo", ["build", "--bin", "sqlrite"], {
    cwd: REPO_ROOT,
    encoding: "utf-8"
  });
  if (build.status !== 0) {
    throw new Error(`cargo build failed: ${build.stdout}\n${build.stderr}`);
  }

  const init = spawnSync(
    sqlriteBin,
    [
      "init",
      "--db",
      dbPath,
      "--seed-demo",
      "--profile",
      "balanced",
      "--index-mode",
      "brute_force"
    ],
    {
      cwd: REPO_ROOT,
      encoding: "utf-8"
    }
  );

  if (init.status !== 0) {
    throw new Error(`sqlrite init failed: ${init.stdout}\n${init.stderr}`);
  }

  const port = await pickFreePort();
  baseUrl = `http://127.0.0.1:${port}`;

  serverProc = spawn(
    sqlriteBin,
    ["serve", "--db", dbPath, "--bind", `127.0.0.1:${port}`],
    {
      cwd: REPO_ROOT,
      stdio: ["ignore", "pipe", "pipe"]
    }
  );

  await waitReady(baseUrl);
  client = new SqlRiteClient(baseUrl);
});

after(() => {
  if (serverProc && !serverProc.killed) {
    serverProc.kill("SIGTERM");
  }
  if (tempDir) {
    rmSync(tempDir, { recursive: true, force: true });
  }
});

test("openapi exposes query endpoint", async () => {
  const openapi = await client.openapi();
  assert.ok(openapi.paths["/v1/query"]);
});

test("query returns rows", async () => {
  const payload = await client.query({ query_text: "agent memory", top_k: 2 });
  assert.equal(payload.kind, "query");
  assert.ok(payload.row_count >= 1);
});

test("sql returns deterministic row count", async () => {
  const payload = await client.sql("SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;");
  assert.equal(payload.kind, "query");
  assert.equal(payload.row_count, 2);
});

test("query validation errors map to typed sdk error", async () => {
  await assert.rejects(
    client.query({ top_k: 2 }),
    (error) =>
      error instanceof SqlRiteApiError &&
      error.statusCode === 400 &&
      String(error.message).includes("query_text or query_embedding")
  );
});
