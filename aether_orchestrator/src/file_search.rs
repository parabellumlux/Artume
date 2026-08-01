//! AetherOS File Search Client
//!
//! Connects to the aetherfs-core gRPC daemon over Unix Domain Socket
//! to perform voice search queries against the file index.
//!
//! This is the bridge between the conversational AI pipeline and the
//! background file indexing daemon.

use anyhow::{Context, Result};
use log::info;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;

use aetherfs_proto::aetherfs::aether_engine_client::AetherEngineClient;
use aetherfs_proto::aetherfs::VoiceSearchRequest;
use aetherfs_proto::aetherfs::IndexConversationRequest;

/// Client for querying the AetherFS file index daemon.
pub struct FileSearchClient {
    /// gRPC channel to the daemon.
    client: Option<AetherEngineClient<Channel>>,
    /// Socket path for Unix Domain Socket connection.
    socket_path: String,
}

/// A single file search result.
#[derive(Debug, Clone)]
pub struct FileSearchResult {
    pub filename: String,
    pub path: String,
    pub classified_type: String,
    pub score: f32,
    pub spoken_summary: String,
    pub temporal_context: String,
    pub location_context: String,
    pub has_duplicates: bool,
}

impl FileSearchClient {
    /// Create a new file search client.
    pub fn new(socket_path: Option<String>) -> Self {
        Self {
            client: None,
            socket_path: socket_path.unwrap_or_else(|| "/tmp/aetherfs.sock".to_string()),
        }
    }

    /// Connect to the aetherfs-core daemon.
    pub async fn connect(&mut self) -> Result<()> {
        let channel = self.connect_ipc().await?;
        self.client = Some(AetherEngineClient::new(channel));
        info!("FileSearchClient: connected to aetherfs-core daemon");
        Ok(())
    }

    /// Search the file index for a query.
    ///
    /// Returns up to `limit` results matching the query text.
    pub async fn search(&mut self, query: &str, limit: usize) -> Result<Vec<FileSearchResult>> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("FileSearchClient not connected — call connect() first"))?;

        let req = VoiceSearchRequest {
            session_id: uuid::Uuid::new_v4().to_string(),
            input: Some(aetherfs_proto::aetherfs::voice_search_request::Input::TextQuery(
                query.to_string(),
            )),
            path_scope: String::new(),
        };

        let request_stream = tokio_stream::iter(vec![req]);

        let response = client
            .live_voice_search(request_stream)
            .await
            .with_context(|| "File search request failed")?;

        let mut response_stream = response.into_inner();
        let mut results = Vec::new();

        while let Some(res) = response_stream.message().await? {
            for m in res.results {
                let (spoken_summary, temporal_context, location_context) = m
                    .conversational_anchor
                    .map(|a| (a.spoken_summary, a.temporal_context, a.location_context))
                    .unwrap_or_default();

                results.push(FileSearchResult {
                    filename: m.filename,
                    path: m.path,
                    classified_type: m.classified_type,
                    score: m.score,
                    spoken_summary,
                    temporal_context,
                    location_context,
                    has_duplicates: m.has_duplicates,
                });
            }
        }

        results.truncate(limit);
        Ok(results)
    }

    /// Index a conversation turn for future RAG retrieval.
    pub async fn index_conversation_turn(
        &mut self,
        session_id: &str,
        user_text: &str,
        assistant_response: &str,
        intent: &str,
    ) -> Result<()> {
        let client = self
            .client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("FileSearchClient not connected — call connect() first"))?;

        let req = IndexConversationRequest {
            session_id: session_id.to_string(),
            user_text: user_text.to_string(),
            assistant_response: assistant_response.to_string(),
            intent: intent.to_string(),
            timestamp_unix: chrono::Utc::now().timestamp(),
        };

        client
            .index_conversation(req)
            .await
            .with_context(|| "Failed to index conversation turn")?;

        Ok(())
    }

    /// Check if the daemon is reachable.
    pub async fn health(&mut self) -> bool {
        if self.client.is_none() {
            if self.connect().await.is_err() {
                return false;
            }
        }
        true
    }

    /// Connect to the daemon via Unix Domain Socket.
    async fn connect_ipc(&self) -> Result<Channel> {
        let path = self.socket_path.clone();
        Endpoint::try_from("http://[::]:50051")?
            .connect_timeout(Duration::from_secs(3))
            .connect_with_connector(service_fn(move |_| {
                let p = path.clone();
                async move { tokio::net::UnixStream::connect(p).await }
            }))
            .await
            .with_context(|| format!("Failed to connect to aetherfs-core at {}", self.socket_path))
    }
}
