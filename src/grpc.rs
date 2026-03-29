use crate::sdk_runtime::{SdkRuntimeError, execute_query, execute_sql};
use crate::{DurabilityProfile, RuntimeConfig, SqlRite, VectorIndexMode};
use serde_json::json;
use sqlrite_sdk_core::{QueryRequest as CoreQueryRequest, SqlRequest as CoreSqlRequest};
use std::net::SocketAddr;
use std::path::PathBuf;
use tonic::{Request, Response, Status};

pub mod proto {
    tonic::include_proto!("sqlrite.v1");
}

use proto::query_service_server::{QueryService, QueryServiceServer};
use proto::{HealthRequest, HealthResponse, QueryRequest, QueryResponse, SqlRequest, SqlResponse};

#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
    pub db_path: PathBuf,
    pub bind_addr: String,
    pub profile: DurabilityProfile,
    pub index_mode: VectorIndexMode,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("sqlrite.db"),
            bind_addr: "127.0.0.1:50051".to_string(),
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
        }
    }
}

#[derive(Debug, Clone)]
struct QueryServiceRuntime {
    db_path: PathBuf,
    profile: DurabilityProfile,
    runtime: RuntimeConfig,
}

impl QueryServiceRuntime {
    fn new(db_path: PathBuf, profile: DurabilityProfile, index_mode: VectorIndexMode) -> Self {
        let runtime = runtime_config(profile, index_mode);
        Self {
            db_path,
            profile,
            runtime,
        }
    }

    #[allow(clippy::result_large_err)]
    fn open_db(&self) -> Result<SqlRite, Status> {
        SqlRite::open_with_config(&self.db_path, self.runtime.clone())
            .map_err(|error| Status::internal(format!("failed to open database: {error}")))
    }

    fn map_runtime_error(error: SdkRuntimeError) -> Status {
        if error.is_validation() {
            Status::invalid_argument(error.to_string())
        } else {
            Status::internal(error.to_string())
        }
    }
}

#[tonic::async_trait]
impl QueryService for QueryServiceRuntime {
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }))
    }

    async fn sql(&self, request: Request<SqlRequest>) -> Result<Response<SqlResponse>, Status> {
        let payload = execute_sql(
            &self.db_path,
            self.profile,
            CoreSqlRequest {
                statement: request.into_inner().statement,
            },
        )
        .map_err(Self::map_runtime_error)?;

        let json_payload = serde_json::to_string(&payload).map_err(|error| {
            Status::internal(format!("failed to encode response json: {error}"))
        })?;

        Ok(Response::new(SqlResponse { json_payload }))
    }

    async fn query(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<QueryResponse>, Status> {
        let input = query_request_from_grpc(request.into_inner());
        let db = self.open_db()?;
        let envelope = execute_query(&db, input).map_err(Self::map_runtime_error)?;

        let json_payload = serde_json::to_string(&envelope).map_err(|error| {
            Status::internal(format!("failed to encode response json: {error}"))
        })?;

        Ok(Response::new(QueryResponse { json_payload }))
    }
}

fn runtime_config(profile: DurabilityProfile, index_mode: VectorIndexMode) -> RuntimeConfig {
    let base = match profile {
        DurabilityProfile::Balanced => RuntimeConfig::default(),
        DurabilityProfile::Durable => RuntimeConfig::durable(),
        DurabilityProfile::FastUnsafe => RuntimeConfig::fast_unsafe(),
    };
    base.with_vector_index_mode(index_mode)
}

fn query_request_from_grpc(input: QueryRequest) -> CoreQueryRequest {
    CoreQueryRequest {
        query_text: input
            .query_text
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        query_embedding: if input.query_embedding.is_empty() {
            None
        } else {
            Some(input.query_embedding)
        },
        top_k: input.top_k.map(|value| value as usize),
        alpha: input.alpha,
        candidate_limit: input.candidate_limit.map(|value| value as usize),
        include_payloads: None,
        query_profile: input.query_profile,
        metadata_filters: if input.metadata_filters.is_empty() {
            None
        } else {
            Some(input.metadata_filters)
        },
        doc_id: input
            .doc_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

pub async fn run_grpc_server(config: GrpcServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = config.bind_addr.parse()?;
    let service = QueryServiceRuntime::new(config.db_path, config.profile, config.index_mode);
    println!("grpc listening on {addr}");
    tonic::transport::Server::builder()
        .add_service(QueryServiceServer::new(service))
        .serve(addr)
        .await?;
    Ok(())
}

pub async fn run_grpc_server_with_shutdown<F>(
    config: GrpcServerConfig,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let addr: SocketAddr = config.bind_addr.parse()?;
    let service = QueryServiceRuntime::new(config.db_path, config.profile, config.index_mode);

    tonic::transport::Server::builder()
        .add_service(QueryServiceServer::new(service))
        .serve_with_shutdown(addr, shutdown)
        .await?;
    Ok(())
}

pub fn grpc_json_payload_or_error(payload: &str) -> serde_json::Value {
    serde_json::from_str(payload)
        .unwrap_or_else(|_| json!({"error": "invalid grpc json payload", "payload": payload}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkInput, RuntimeConfig};
    use serde_json::Value;
    use tempfile::NamedTempFile;
    use tokio::sync::oneshot;

    async fn start_test_server(
        db_path: PathBuf,
        bind_addr: String,
    ) -> Result<oneshot::Sender<()>, Box<dyn std::error::Error>> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let config = GrpcServerConfig {
            db_path,
            bind_addr,
            profile: DurabilityProfile::Balanced,
            index_mode: VectorIndexMode::BruteForce,
        };

        tokio::spawn(async move {
            let _ = run_grpc_server_with_shutdown(config, async move {
                let _ = shutdown_rx.await;
            })
            .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        Ok(shutdown_tx)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn grpc_health_and_query_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path().to_path_buf();
        let db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;
        db.ingest_chunk(&ChunkInput {
            id: "grpc-q-1".to_string(),
            doc_id: "doc-1".to_string(),
            content: "grpc query service".to_string(),
            embedding: vec![1.0, 0.0],
            metadata: json!({"tenant": "demo"}),
            source: None,
        })?;

        let bind_addr = "127.0.0.1:50081".to_string();
        let shutdown = start_test_server(db_path.clone(), bind_addr.clone()).await?;

        let endpoint = format!("http://{bind_addr}");
        let mut client = proto::query_service_client::QueryServiceClient::connect(endpoint).await?;

        let health = client
            .health(Request::new(HealthRequest {}))
            .await?
            .into_inner();
        assert_eq!(health.status, "ok");

        let query_response = client
            .query(Request::new(QueryRequest {
                query_text: Some("grpc".to_string()),
                query_embedding: vec![],
                top_k: Some(1),
                alpha: None,
                candidate_limit: Some(10),
                query_profile: None,
                metadata_filters: Default::default(),
                doc_id: None,
            }))
            .await?
            .into_inner();
        let payload: Value = serde_json::from_str(&query_response.json_payload)?;
        assert_eq!(payload["kind"], "query");
        assert_eq!(payload["row_count"], 1);

        let sql_response = client
            .sql(Request::new(SqlRequest {
                statement: "SELECT id FROM chunks ORDER BY id ASC LIMIT 1;".to_string(),
            }))
            .await?
            .into_inner();
        let payload: Value = serde_json::from_str(&sql_response.json_payload)?;
        assert_eq!(payload["kind"], "query");
        assert_eq!(payload["row_count"], 1);

        let _ = shutdown.send(());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn grpc_query_rejects_missing_query_inputs() -> Result<(), Box<dyn std::error::Error>> {
        let db_file = NamedTempFile::new()?;
        let db_path = db_file.path().to_path_buf();
        let _db = SqlRite::open_with_config(&db_path, RuntimeConfig::default())?;

        let bind_addr = "127.0.0.1:50082".to_string();
        let shutdown = start_test_server(db_path.clone(), bind_addr.clone()).await?;

        let endpoint = format!("http://{bind_addr}");
        let mut client = proto::query_service_client::QueryServiceClient::connect(endpoint).await?;

        let error = client
            .query(Request::new(QueryRequest {
                query_text: None,
                query_embedding: vec![],
                top_k: Some(1),
                alpha: None,
                candidate_limit: Some(1),
                query_profile: None,
                metadata_filters: Default::default(),
                doc_id: None,
            }))
            .await
            .expect_err("expected invalid argument");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        let _ = shutdown.send(());
        Ok(())
    }
}
