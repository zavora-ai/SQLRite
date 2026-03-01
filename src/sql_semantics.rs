use crate::DurabilityProfile;
use rusqlite::Connection;
use rusqlite::Error as SqlError;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub fn prepare_sql_connection(
    conn: &Connection,
    profile: DurabilityProfile,
) -> Result<(), SqlError> {
    apply_sql_runtime_profile(conn, profile)?;
    register_retrieval_sql_functions(conn)?;
    Ok(())
}

pub fn execute_sql_statement_json(conn: &Connection, statement: &str) -> Result<Value, SqlError> {
    let start = Instant::now();
    let rewritten = rewrite_sql_vector_operators(statement);

    if is_query_statement(&rewritten) {
        let mut stmt = conn.prepare(&rewritten)?;
        let column_count = stmt.column_count();
        let column_names = stmt
            .column_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();

        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let mut object = Map::new();
            for idx in 0..column_count {
                let key = column_names
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{idx}"));
                let value = row.get_ref(idx)?;
                object.insert(key, sql_value_to_json(value));
            }
            out.push(Value::Object(object));
        }

        return Ok(json!({
            "kind": "query",
            "statement": statement,
            "rewritten_statement": rewritten,
            "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
            "rows": out,
            "row_count": out.len(),
        }));
    }

    let before = conn.total_changes();
    conn.execute_batch(&rewritten)?;
    let after = conn.total_changes();
    let rows_affected = after.saturating_sub(before);

    Ok(json!({
        "kind": "mutation",
        "statement": statement,
        "rewritten_statement": rewritten,
        "elapsed_ms": start.elapsed().as_secs_f64() * 1000.0,
        "rows_affected": rows_affected,
        "last_insert_rowid": conn.last_insert_rowid(),
    }))
}

fn sql_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(v) => json!(v),
        ValueRef::Real(v) => json!(v),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Blob(bytes) => Value::String(format!("blob:{}bytes", bytes.len())),
    }
}

fn apply_sql_runtime_profile(
    conn: &Connection,
    profile: DurabilityProfile,
) -> Result<(), SqlError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", synchronous_sql(profile))?;
    let _: String = conn.query_row("PRAGMA journal_mode = WAL;", [], |row| row.get(0))?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

fn synchronous_sql(profile: DurabilityProfile) -> &'static str {
    match profile {
        DurabilityProfile::Balanced => "NORMAL",
        DurabilityProfile::Durable => "FULL",
        DurabilityProfile::FastUnsafe => "OFF",
    }
}

fn register_retrieval_sql_functions(conn: &Connection) -> Result<(), SqlError> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;

    conn.create_scalar_function("vector", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        Ok(encode_vector_blob(&vector))
    })?;

    conn.create_scalar_function("vec_dims", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        Ok(vector.len() as i64)
    })?;

    conn.create_scalar_function("vec_to_json", 1, flags, |ctx| {
        let vector = vector_from_value(ctx.get_raw(0))?;
        serde_json::to_string(&vector)
            .map_err(|error| user_fn_error(format!("failed to serialize vector: {error}")))
    })?;

    conn.create_scalar_function("l2_distance", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(l2_distance(&left, &right) as f64)
    })?;

    conn.create_scalar_function("cosine_distance", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(cosine_distance(&left, &right) as f64)
    })?;

    conn.create_scalar_function("neg_inner_product", 2, flags, |ctx| {
        let left = vector_from_value(ctx.get_raw(0))?;
        let right = vector_from_value(ctx.get_raw(1))?;
        ensure_same_dimension(&left, &right)?;
        Ok(neg_inner_product(&left, &right) as f64)
    })?;

    conn.create_scalar_function("embed", 1, flags, |ctx| {
        let text = text_from_value(ctx.get_raw(0))?;
        let vector = embed_text(&text, 16);
        Ok(encode_vector_blob(&vector))
    })?;

    conn.create_scalar_function("bm25_score", 2, flags, |ctx| {
        let query = text_from_value(ctx.get_raw(0))?;
        let document = text_from_value(ctx.get_raw(1))?;
        Ok(bm25_score(&query, &document) as f64)
    })?;

    conn.create_scalar_function("hybrid_score", 3, flags, |ctx| {
        let vector_score = ctx.get::<f64>(0)?;
        let text_score = ctx.get::<f64>(1)?;
        let alpha = ctx.get::<f64>(2)?;
        if !(0.0..=1.0).contains(&alpha) {
            return Err(user_fn_error(
                "hybrid_score alpha must be between 0.0 and 1.0",
            ));
        }
        Ok((alpha * vector_score) + ((1.0 - alpha) * text_score))
    })?;

    Ok(())
}

fn user_fn_error(message: impl Into<String>) -> SqlError {
    SqlError::UserFunctionError(Box::new(std::io::Error::other(message.into())))
}

fn ensure_same_dimension(left: &[f32], right: &[f32]) -> Result<(), SqlError> {
    if left.len() == right.len() {
        return Ok(());
    }
    Err(user_fn_error(format!(
        "vector dimension mismatch: left={} right={}",
        left.len(),
        right.len()
    )))
}

fn vector_from_value(value: ValueRef<'_>) -> Result<Vec<f32>, SqlError> {
    match value {
        ValueRef::Blob(bytes) => decode_vector_blob(bytes),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            parse_vector_literal(&text).map_err(user_fn_error)
        }
        ValueRef::Integer(v) => Ok(vec![v as f32]),
        ValueRef::Real(v) => Ok(vec![v as f32]),
        ValueRef::Null => Err(user_fn_error(
            "vector argument cannot be NULL; expected BLOB or text literal",
        )),
    }
}

fn text_from_value(value: ValueRef<'_>) -> Result<String, SqlError> {
    match value {
        ValueRef::Text(bytes) => Ok(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Blob(bytes) => Ok(format!("blob:{}bytes", bytes.len())),
        ValueRef::Integer(v) => Ok(v.to_string()),
        ValueRef::Real(v) => Ok(v.to_string()),
        ValueRef::Null => Err(user_fn_error("text argument cannot be NULL")),
    }
}

fn tokenize_terms(value: &str) -> Vec<String> {
    value
        .to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_vector_literal(raw: &str) -> Result<Vec<f32>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty vector literal".to_string());
    }

    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    let values = inner
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<f32>()
                .map_err(|_| format!("invalid vector element `{token}`"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if values.is_empty() {
        return Err("vector literal must contain at least one value".to_string());
    }

    Ok(values)
}

fn encode_vector_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_vector_blob(bytes: &[u8]) -> Result<Vec<f32>, SqlError> {
    if bytes.is_empty() {
        return Err(user_fn_error("vector blob cannot be empty"));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(user_fn_error(format!(
            "invalid vector blob byte length {}; expected multiple of 4",
            bytes.len()
        )));
    }

    let mut out = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(out)
}

fn l2_distance(left: &[f32], right: &[f32]) -> f32 {
    left.iter()
        .zip(right.iter())
        .map(|(a, b)| {
            let delta = a - b;
            delta * delta
        })
        .sum::<f32>()
        .sqrt()
}

fn cosine_distance(left: &[f32], right: &[f32]) -> f32 {
    let dot = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (left_norm * right_norm))
}

fn neg_inner_product(left: &[f32], right: &[f32]) -> f32 {
    -left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>()
}

fn embed_text(text: &str, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim.max(1)];
    let terms = tokenize_terms(text);
    if terms.is_empty() {
        out[0] = 1.0;
        return out;
    }

    for (position, term) in terms.iter().enumerate() {
        let hash = fnv1a64(term.as_bytes());
        let slot = (hash as usize) % out.len();
        let sign = if hash & 1 == 0 { 1.0 } else { -1.0 };
        let weight = 1.0 / ((position + 1) as f32).sqrt();
        out[slot] += sign * weight;
    }

    let norm = out.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut out {
            *value /= norm;
        }
    }

    out
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn bm25_score(query: &str, document: &str) -> f32 {
    let query_terms = tokenize_terms(query);
    let doc_terms = tokenize_terms(document);
    if query_terms.is_empty() || doc_terms.is_empty() {
        return 0.0;
    }

    let mut tf: HashMap<String, usize> = HashMap::new();
    for term in &doc_terms {
        *tf.entry(term.clone()).or_insert(0) += 1;
    }

    let mut unique_query_terms = HashSet::new();
    let dl = doc_terms.len() as f32;
    let avgdl = 50.0f32;
    let k1 = 1.2f32;
    let b = 0.75f32;
    let mut score = 0.0f32;

    for term in query_terms {
        if !unique_query_terms.insert(term.clone()) {
            continue;
        }

        let tf_value = tf.get(&term).copied().unwrap_or(0) as f32;
        if tf_value == 0.0 {
            continue;
        }

        let idf = ((1.0 + (1.0 / (tf_value + 1.0))).ln() + 1.0).max(0.01);
        let denominator = tf_value + k1 * (1.0 - b + b * (dl / avgdl));
        score += idf * (tf_value * (k1 + 1.0)) / denominator;
    }

    score
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VectorOperator {
    L2,
    Cosine,
    NegInner,
}

pub fn rewrite_sql_vector_operators(statement: &str) -> String {
    let mut rewritten = statement.to_string();
    for _ in 0..512 {
        let Some((operator_start, operator_end, operator)) = find_next_vector_operator(&rewritten)
        else {
            break;
        };
        let Some(left_start) = find_left_operand_start(&rewritten, operator_start) else {
            break;
        };
        let Some(right_end) = find_right_operand_end(&rewritten, operator_end) else {
            break;
        };

        let left_expr = rewritten[left_start..operator_start].trim();
        let right_expr = rewritten[operator_end..right_end].trim();
        if left_expr.is_empty() || right_expr.is_empty() {
            break;
        }

        let replacement = format!(
            "{}({}, {})",
            vector_operator_function(operator),
            left_expr,
            right_expr
        );
        rewritten = format!(
            "{}{}{}",
            &rewritten[..left_start],
            replacement,
            &rewritten[right_end..]
        );
    }
    rewritten
}

fn vector_operator_function(operator: VectorOperator) -> &'static str {
    match operator {
        VectorOperator::L2 => "l2_distance",
        VectorOperator::Cosine => "cosine_distance",
        VectorOperator::NegInner => "neg_inner_product",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanState {
    Normal,
    SingleQuoted,
    DoubleQuoted,
    LineComment,
    BlockComment,
}

fn find_next_vector_operator(statement: &str) -> Option<(usize, usize, VectorOperator)> {
    let bytes = statement.as_bytes();
    let mut i = 0usize;
    let mut state = ScanState::Normal;
    while i < bytes.len() {
        match state {
            ScanState::Normal => {
                if bytes[i] == b'\'' {
                    state = ScanState::SingleQuoted;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'"' {
                    state = ScanState::DoubleQuoted;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    state = ScanState::LineComment;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    state = ScanState::BlockComment;
                    i += 2;
                    continue;
                }
                if bytes[i] == b'<' && i + 2 < bytes.len() {
                    if bytes[i + 1] == b'-' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::L2));
                    }
                    if bytes[i + 1] == b'=' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::Cosine));
                    }
                    if bytes[i + 1] == b'#' && bytes[i + 2] == b'>' {
                        return Some((i, i + 3, VectorOperator::NegInner));
                    }
                }
                i += 1;
            }
            ScanState::SingleQuoted => {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                    } else {
                        state = ScanState::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[i] == b'"' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                        i += 2;
                    } else {
                        state = ScanState::Normal;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            ScanState::LineComment => {
                if bytes[i] == b'\n' {
                    state = ScanState::Normal;
                }
                i += 1;
            }
            ScanState::BlockComment => {
                if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    state = ScanState::Normal;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        }
    }
    None
}

fn find_left_operand_start(statement: &str, operator_start: usize) -> Option<usize> {
    let bytes = statement.as_bytes();
    let mut end = operator_start;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 {
        return None;
    }

    let last = bytes[end - 1];
    if last == b')' {
        let open = seek_balanced_backward(bytes, end - 1, b'(', b')')?;
        let mut start = open;
        while start > 0 && is_token_char(bytes[start - 1]) {
            start -= 1;
        }
        return Some(start);
    }
    if last == b']' {
        return seek_balanced_backward(bytes, end - 1, b'[', b']');
    }
    if last == b'\'' || last == b'"' {
        return seek_quoted_start(bytes, end - 1, last);
    }

    let mut start = end;
    while start > 0 && !is_left_boundary(bytes[start - 1]) {
        start -= 1;
    }
    Some(start)
}

fn find_right_operand_end(statement: &str, operator_end: usize) -> Option<usize> {
    let bytes = statement.as_bytes();
    let mut start = operator_end;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }

    match bytes[start] {
        b'(' => return seek_balanced_forward(bytes, start, b'(', b')').map(|idx| idx + 1),
        b'[' => return seek_balanced_forward(bytes, start, b'[', b']').map(|idx| idx + 1),
        b'\'' | b'"' => {
            return seek_quoted_end(bytes, start).map(|idx| idx + 1);
        }
        _ => {}
    }

    if is_token_char(bytes[start]) {
        let mut token_end = start;
        while token_end < bytes.len() && is_token_char(bytes[token_end]) {
            token_end += 1;
        }
        let mut probe = token_end;
        while probe < bytes.len() && bytes[probe].is_ascii_whitespace() {
            probe += 1;
        }
        if probe < bytes.len()
            && bytes[probe] == b'('
            && is_callable_token(&bytes[start..token_end])
            && let Some(close) = seek_balanced_forward(bytes, probe, b'(', b')')
        {
            return Some(close + 1);
        }
        return Some(token_end);
    }

    let mut end = start;
    while end < bytes.len() && !is_right_boundary(bytes[end]) {
        end += 1;
    }

    Some(end)
}

fn seek_balanced_backward(bytes: &[u8], close_index: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut idx = close_index;
    loop {
        let current = bytes[idx];
        if current == close {
            depth += 1;
        } else if current == open {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

fn seek_balanced_forward(bytes: &[u8], open_index: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut idx = open_index;
    let mut state = ScanState::Normal;
    while idx < bytes.len() {
        match state {
            ScanState::Normal => {
                if bytes[idx] == b'\'' {
                    state = ScanState::SingleQuoted;
                    idx += 1;
                    continue;
                }
                if bytes[idx] == b'"' {
                    state = ScanState::DoubleQuoted;
                    idx += 1;
                    continue;
                }
                if bytes[idx] == open {
                    depth += 1;
                } else if bytes[idx] == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                idx += 1;
            }
            ScanState::SingleQuoted => {
                if bytes[idx] == b'\'' {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                        idx += 2;
                    } else {
                        state = ScanState::Normal;
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
            }
            ScanState::DoubleQuoted => {
                if bytes[idx] == b'"' {
                    if idx + 1 < bytes.len() && bytes[idx + 1] == b'"' {
                        idx += 2;
                    } else {
                        state = ScanState::Normal;
                        idx += 1;
                    }
                } else {
                    idx += 1;
                }
            }
            ScanState::LineComment | ScanState::BlockComment => {
                idx += 1;
            }
        }
    }
    None
}

fn seek_quoted_end(bytes: &[u8], quote_start: usize) -> Option<usize> {
    let quote = bytes[quote_start];
    let mut idx = quote_start + 1;
    while idx < bytes.len() {
        if bytes[idx] == quote {
            if idx + 1 < bytes.len() && bytes[idx + 1] == quote {
                idx += 2;
                continue;
            }
            return Some(idx);
        }
        idx += 1;
    }
    None
}

fn seek_quoted_start(bytes: &[u8], quote_end: usize, quote: u8) -> Option<usize> {
    let mut idx = quote_end;
    loop {
        if bytes[idx] == quote {
            if idx > 0 && bytes[idx - 1] == quote {
                if idx < 2 {
                    return None;
                }
                idx -= 2;
                continue;
            }
            return Some(idx);
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

fn is_left_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b';'
                | b'('
                | b')'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'!'
                | b'|'
                | b'&'
                | b'^'
        )
}

fn is_right_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b',' | b';'
                | b')'
                | b'+'
                | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b'='
                | b'<'
                | b'>'
                | b'!'
                | b'|'
                | b'&'
                | b'^'
        )
}

fn is_token_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$')
}

fn is_callable_token(token: &[u8]) -> bool {
    !token.is_empty() && token.iter().all(|byte| is_token_char(*byte))
}

fn is_query_statement(sql: &str) -> bool {
    let normalized = sql.trim_start().to_ascii_lowercase();
    normalized.starts_with("select")
        || normalized.starts_with("with")
        || normalized.starts_with("pragma")
        || normalized.starts_with("explain")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_operator_rewrite_maps_operators() {
        let sql = "SELECT embedding <-> vector('1,0,0') AS l2, embedding <=> vector('1,0,0') AS c, embedding <#> vector('1,0,0') AS nip FROM chunks;";
        let rewritten = rewrite_sql_vector_operators(sql);
        assert!(rewritten.contains("l2_distance(embedding, vector('1,0,0'))"));
        assert!(rewritten.contains("cosine_distance(embedding, vector('1,0,0'))"));
        assert!(rewritten.contains("neg_inner_product(embedding, vector('1,0,0'))"));
    }

    #[test]
    fn prepare_and_execute_query_json() -> Result<(), Box<dyn std::error::Error>> {
        let conn = Connection::open_in_memory()?;
        prepare_sql_connection(&conn, DurabilityProfile::Balanced)?;
        conn.execute_batch(
            "
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                content TEXT NOT NULL
            );
            INSERT INTO chunks (id, embedding, content)
            VALUES ('c1', vector('1,0,0'), 'agent memory chunk');
            ",
        )?;
        let payload = execute_sql_statement_json(
            &conn,
            "SELECT id, embedding <=> vector('1,0,0') AS d FROM chunks ORDER BY d ASC, id ASC;",
        )?;

        assert_eq!(payload["kind"], "query");
        assert_eq!(payload["row_count"], 1);
        assert_eq!(payload["rows"][0]["id"], "c1");
        Ok(())
    }
}
