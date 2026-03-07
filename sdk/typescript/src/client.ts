export interface QueryRequest {
  query_text?: string;
  query_embedding?: number[];
  top_k?: number;
  alpha?: number;
  candidate_limit?: number;
  query_profile?: "balanced" | "latency" | "recall";
  metadata_filters?: Record<string, string>;
  doc_id?: string;
}

export interface QueryEnvelope<T = unknown> {
  kind: string;
  row_count: number;
  rows: T[];
}

export class SqlRiteApiError extends Error {
  statusCode: number;
  payload: unknown;

  constructor(statusCode: number, message: string, payload: unknown = null) {
    super(`sqlrite api error (status=${statusCode}): ${message}`);
    this.name = "SqlRiteApiError";
    this.statusCode = statusCode;
    this.payload = payload;
  }
}

export class SqlRiteClient {
  private readonly baseUrl: string;
  private readonly timeoutMs: number;

  constructor(baseUrl = "http://127.0.0.1:8099", timeoutMs = 10_000) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.timeoutMs = timeoutMs;
  }

  health<T = unknown>(): Promise<T> {
    return this.requestJson<T>("GET", "/healthz");
  }

  ready<T = unknown>(): Promise<T> {
    return this.requestJson<T>("GET", "/readyz");
  }

  openapi<T = unknown>(): Promise<T> {
    return this.requestJson<T>("GET", "/v1/openapi.json");
  }

  sql<T = unknown>(statement: string): Promise<T> {
    return this.requestJson<T>("POST", "/v1/sql", { statement });
  }

  query<T = unknown>(request: QueryRequest): Promise<QueryEnvelope<T>> {
    return this.requestJson<QueryEnvelope<T>>("POST", "/v1/query", request);
  }

  private async requestJson<T>(method: string, path: string, body?: unknown): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    const headers: Record<string, string> = {
      accept: "application/json"
    };

    const requestInit: RequestInit = {
      method,
      headers,
      signal: controller.signal
    };

    if (body !== undefined) {
      headers["content-type"] = "application/json";
      requestInit.body = JSON.stringify(body);
    }

    try {
      const response = await fetch(`${this.baseUrl}${path}`, requestInit);
      const text = await response.text();
      const payload = text.trim().length > 0 ? JSON.parse(text) : {};

      if (!response.ok) {
        const message =
          typeof payload?.error === "string"
            ? payload.error
            : `request failed for ${method} ${path}`;
        throw new SqlRiteApiError(response.status, message, payload);
      }

      return payload as T;
    } catch (error) {
      if (error instanceof SqlRiteApiError) {
        throw error;
      }
      throw new SqlRiteApiError(0, `connection error: ${String(error)}`);
    } finally {
      clearTimeout(timer);
    }
  }
}
