use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CreateCollection, Distance, PointId, PointStruct, SearchPoints, VectorParams, VectorsConfig,
    UpsertPointsBuilder,
};
use qdrant_client::qdrant::vectors_config::Config;
use std::sync::Arc;
use uuid::Uuid;

pub struct QdrantIndex {
    client: Option<Arc<Qdrant>>,
    collection_name: String,
}

impl QdrantIndex {
    /// Create Qdrant index connector.
    pub fn new(url: &str, collection_name: &str) -> Self {
        // Build the Qdrant client asynchronously or try to build it
        let client = match Qdrant::from_url(url).build() {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                eprintln!("AetherFS Qdrant: Failed to build Qdrant Client at {}: {}", url, e);
                None
            }
        };

        Self {
            client,
            collection_name: collection_name.to_string(),
        }
    }

    /// Asynchronously initialize collection in Qdrant if it doesn't exist.
    pub async fn init_collection(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = match &self.client {
            Some(c) => c,
            None => return Err("Qdrant client not initialized".into()),
        };

        let has_collection = client.collection_exists(&self.collection_name).await.unwrap_or(false);
        if !has_collection {
            println!("AetherFS Qdrant: Collection '{}' not found, creating it...", self.collection_name);
            client
                .create_collection(CreateCollection {
                    collection_name: self.collection_name.clone(),
                    vectors_config: Some(VectorsConfig {
                        config: Some(Config::Params(VectorParams {
                            size: 384, // MiniLM-L6-v2 dimension
                            distance: Distance::Cosine.into(),
                            ..Default::default()
                        })),
                    }),
                    ..Default::default()
                })
                .await?;
            println!("AetherFS Qdrant: Collection '{}' created successfully", self.collection_name);
        } else {
            println!("AetherFS Qdrant: Collection '{}' verified", self.collection_name);
        }

        Ok(())
    }

    /// Insert or update vector embedding for a path.
    pub async fn upsert_vector(
        &self,
        path: &str,
        embedding: Vec<f32>,
        classified_type: &str,
        spoken_summary: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = match &self.client {
            Some(c) => c,
            None => return Err("Qdrant client not initialized".into()),
        };

        // Create a unique deterministic UUID based on the path
        let point_uuid = Uuid::new_v5(&Uuid::NAMESPACE_URL, path.as_bytes()).to_string();

        let payload_json = serde_json::json!({
            "path": path,
            "classified_type": classified_type,
            "spoken_summary": spoken_summary
        });
        let payload: qdrant_client::Payload = payload_json.try_into()?;

        let point = PointStruct::new(
            PointId::from(point_uuid),
            embedding,
            payload,
        );

        client
            .upsert_points(UpsertPointsBuilder::new(&self.collection_name, vec![point]))
            .await?;

        Ok(())
    }

    /// Search vector space using query embedding.
    pub async fn search_vector(
        &self,
        query_embedding: Vec<f32>,
        limit: u64,
    ) -> Result<Vec<(String, f32, String)>, Box<dyn std::error::Error + Send + Sync>> {
        let client = match &self.client {
            Some(c) => c,
            None => return Err("Qdrant client not initialized".into()),
        };

        let search_query = SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: query_embedding,
            limit,
            with_payload: Some(true.into()),
            ..Default::default()
        };

        let response = client.search_points(search_query).await?;
        let mut results = Vec::new();

        for point in response.result {
            let score = point.score;
            let payload = point.payload;
            
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let spoken_summary = payload
                .get("spoken_summary")
                .and_then(|v| v.as_str())
                .map(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            if !path.is_empty() {
                results.push((path, score, spoken_summary));
            }
        }

        Ok(results)
    }
}
