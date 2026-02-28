#!/usr/bin/env bash

set -euo pipefail

DB_PATH="${1:-s07_cookbook.db}"
LOG_PATH="${2:-project_plan/reports/s07_sql_conformance.log}"
SUMMARY_PATH="${3:-project_plan/reports/s07_sql_conformance.json}"

mkdir -p "$(dirname "$LOG_PATH")"
mkdir -p "$(dirname "$SUMMARY_PATH")"
rm -f "$DB_PATH" "$LOG_PATH" "$SUMMARY_PATH"

log() {
  echo "$1" | tee -a "$LOG_PATH"
}

run() {
  log "\$ $*"
  "$@" | tee -a "$LOG_PATH"
  log ""
}

run_sql_case() {
  local name="$1"
  local sql="$2"

  log "pattern=$name"
  if cargo run -- sql --db "$DB_PATH" --execute "$sql" >>"$LOG_PATH" 2>&1; then
    PASSED_PATTERNS+=("$name")
    log "status=passed"
  else
    FAILED_PATTERNS+=("$name")
    log "status=failed"
  fi
  log ""
}

declare -a PASSED_PATTERNS=()
declare -a FAILED_PATTERNS=()

run cargo run -- init --db "$DB_PATH" --seed-demo --profile balanced --index-mode brute_force

run_sql_case "vector_ddl" \
  "CREATE VECTOR INDEX IF NOT EXISTS idx_chunks_embedding_hnsw ON chunks(embedding) USING HNSW WITH (m=16, ef_construction=64);"
run_sql_case "text_ddl" \
  "CREATE TEXT INDEX IF NOT EXISTS idx_chunks_content_fts ON chunks(content) USING FTS5 WITH (tokenizer=unicode61);"

run_sql_case "semantic_vector" \
  "SELECT id, embedding <-> vector('0.95,0.05,0.0') AS l2 FROM chunks ORDER BY l2 ASC, id ASC LIMIT 5;"
run_sql_case "lexical" \
  "SELECT c.id, c.doc_id, bm25(chunks_fts) AS rank FROM chunks_fts JOIN chunks AS c ON c.id = chunks_fts.chunk_id WHERE chunks_fts MATCH 'local OR agent' ORDER BY rank ASC, c.id ASC LIMIT 5;"
run_sql_case "hybrid" \
  "SELECT id, 1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score, bm25_score('local agent memory', content) AS text_score, hybrid_score(1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')), bm25_score('local agent memory', content), 0.65) AS hybrid FROM chunks ORDER BY hybrid DESC, id ASC LIMIT 5;"
run_sql_case "tenant_filter" \
  "SELECT id, doc_id, content FROM chunks WHERE json_extract(metadata, '$.tenant') = 'demo' ORDER BY id ASC LIMIT 10;"
run_sql_case "topic_filter" \
  "SELECT id, doc_id, content FROM chunks WHERE json_extract(metadata, '$.topic') = 'retrieval' ORDER BY id ASC LIMIT 10;"
run_sql_case "doc_scope" \
  "SELECT id, doc_id, content FROM chunks WHERE doc_id = 'doc-a' ORDER BY id ASC LIMIT 10;"
run_sql_case "rerank_ready" \
  "SELECT id, content, 1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score, bm25_score('local agent memory', content) AS text_score FROM chunks ORDER BY vector_score DESC, text_score DESC, id ASC LIMIT 20;"
run_sql_case "explain_retrieval" \
  "EXPLAIN RETRIEVAL SELECT id, 1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')) AS vector_score, bm25_score('local agent memory', content) AS text_score, hybrid_score(1.0 - cosine_distance(embedding, vector('0.95,0.05,0.0')), bm25_score('local agent memory', content), 0.65) AS hybrid FROM chunks ORDER BY hybrid DESC, id ASC LIMIT 5;"
run_sql_case "index_catalog" \
  "SELECT name, index_kind, table_name, column_name, using_engine, status FROM retrieval_index_catalog ORDER BY name;"

TOTAL_PATTERNS=$(( ${#PASSED_PATTERNS[@]} + ${#FAILED_PATTERNS[@]} ))
PASS_RATE=0
if [[ $TOTAL_PATTERNS -gt 0 ]]; then
  PASS_RATE=$(awk "BEGIN { printf \"%.6f\", ${#PASSED_PATTERNS[@]} / $TOTAL_PATTERNS }")
fi

{
  echo "{"
  echo "  \"db_path\": \"${DB_PATH}\","
  echo "  \"total_patterns\": ${TOTAL_PATTERNS},"
  echo "  \"passed_patterns\": ${#PASSED_PATTERNS[@]},"
  echo "  \"failed_patterns\": ${#FAILED_PATTERNS[@]},"
  echo "  \"pass_rate\": ${PASS_RATE},"
  echo "  \"passed\": ["
  for i in "${!PASSED_PATTERNS[@]}"; do
    if [[ $i -gt 0 ]]; then
      echo ","
    fi
    printf "    \"%s\"" "${PASSED_PATTERNS[$i]}"
  done
  echo ""
  echo "  ],"
  echo "  \"failed\": ["
  for i in "${!FAILED_PATTERNS[@]}"; do
    if [[ $i -gt 0 ]]; then
      echo ","
    fi
    printf "    \"%s\"" "${FAILED_PATTERNS[$i]}"
  done
  echo ""
  echo "  ]"
  echo "}"
} >"$SUMMARY_PATH"

log "summary_path=$SUMMARY_PATH"
log "total_patterns=$TOTAL_PATTERNS passed=${#PASSED_PATTERNS[@]} failed=${#FAILED_PATTERNS[@]} pass_rate=$PASS_RATE"

if [[ ${#FAILED_PATTERNS[@]} -gt 0 ]]; then
  exit 1
fi
