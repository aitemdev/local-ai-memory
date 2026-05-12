use anyhow::{Result, anyhow};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, RecordBatchReader,
    StringArray, types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::{
    Connection, DistanceType, Table, connect,
    query::{ExecutableQuery, QueryBase},
};
use std::{path::Path, sync::Arc};
use tokio::runtime::{Builder, Runtime};
use futures::TryStreamExt;

pub struct VectorStore {
    runtime: Runtime,
    connection: Connection,
}

#[derive(Debug, Clone)]
pub struct VectorRow {
    pub chunk_id: String,
    pub document_id: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct VectorHit {
    pub chunk_id: String,
    #[allow(dead_code)]
    pub document_id: String,
    pub distance: f32,
}

impl VectorStore {
    pub fn open(base: &Path) -> Result<Self> {
        let dir = base.join("lance");
        std::fs::create_dir_all(&dir)?;
        let runtime = Builder::new_current_thread().enable_all().build()?;
        let uri = dir.to_string_lossy().to_string();
        let connection = runtime.block_on(async { connect(&uri).execute().await })?;
        Ok(Self { runtime, connection })
    }

    pub fn upsert(&self, table_name: &str, dim: usize, rows: &[VectorRow]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let safe = sanitize(table_name);
        let schema = make_schema(dim);
        self.runtime.block_on(async {
            let table = self.open_or_create(&safe, schema.clone(), dim).await?;
            let batch = build_batch(schema.clone(), rows, dim)?;
            let chunk_ids: Vec<String> = rows.iter().map(|r| r.chunk_id.clone()).collect();
            let predicate = chunk_ids
                .iter()
                .map(|id| format!("'{}'", id.replace('\'', "''")))
                .collect::<Vec<_>>()
                .join(",");
            if !predicate.is_empty() {
                let _ = table.delete(&format!("chunk_id IN ({})", predicate)).await;
            }
            let reader = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
            let boxed: Box<dyn RecordBatchReader + Send> = Box::new(reader);
            table.add(boxed).execute().await?;
            Ok::<(), anyhow::Error>(())
        })
    }

    pub fn query(&self, table_name: &str, vector: &[f32], limit: usize) -> Result<Vec<VectorHit>> {
        let safe = sanitize(table_name);
        self.runtime.block_on(async {
            let names = self.connection.table_names().execute().await?;
            if !names.contains(&safe) {
                return Ok::<Vec<VectorHit>, anyhow::Error>(Vec::new());
            }
            let table = self.connection.open_table(&safe).execute().await?;
            let mut stream = table
                .query()
                .nearest_to(vector.to_vec())?
                .distance_type(DistanceType::Cosine)
                .limit(limit)
                .execute()
                .await?;
            let mut hits = Vec::new();
            while let Some(batch) = stream.try_next().await? {
                extract_hits(&batch, &mut hits)?;
            }
            Ok(hits)
        })
    }

    pub fn delete_document(&self, document_id: &str) -> Result<()> {
        let escaped = document_id.replace('\'', "''");
        self.runtime.block_on(async {
            let names = self.connection.table_names().execute().await?;
            for name in names {
                let table = self.connection.open_table(&name).execute().await?;
                let _ = table.delete(&format!("document_id = '{escaped}'")).await;
            }
            Ok::<(), anyhow::Error>(())
        })
    }

    async fn open_or_create(&self, name: &str, schema: Arc<Schema>, _dim: usize) -> Result<Table> {
        let names = self.connection.table_names().execute().await?;
        if names.contains(&name.to_string()) {
            Ok(self.connection.open_table(name).execute().await?)
        } else {
            let empty = RecordBatchIterator::new(std::iter::empty(), schema.clone());
            let boxed: Box<dyn RecordBatchReader + Send> = Box::new(empty);
            Ok(self
                .connection
                .create_table(name, boxed)
                .execute()
                .await?)
        }
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn make_schema(dim: usize) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("document_id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
    ]))
}

fn build_batch(schema: Arc<Schema>, rows: &[VectorRow], dim: usize) -> Result<RecordBatch> {
    for row in rows {
        if row.vector.len() != dim {
            return Err(anyhow!(
                "vector length {} does not match table dim {}",
                row.vector.len(),
                dim
            ));
        }
    }
    let chunk_ids = StringArray::from(rows.iter().map(|r| r.chunk_id.as_str()).collect::<Vec<_>>());
    let document_ids = StringArray::from(rows.iter().map(|r| r.document_id.as_str()).collect::<Vec<_>>());
    let flat: Vec<Option<f32>> = rows
        .iter()
        .flat_map(|r| r.vector.iter().map(|v| Some(*v)))
        .collect();
    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        rows.iter()
            .map(|r| Some(r.vector.iter().map(|v| Some(*v)).collect::<Vec<_>>())),
        dim as i32,
    );
    let _ = flat;
    Ok(RecordBatch::try_new(
        schema,
        vec![Arc::new(chunk_ids), Arc::new(document_ids), Arc::new(vectors)],
    )?)
}

fn extract_hits(batch: &RecordBatch, hits: &mut Vec<VectorHit>) -> Result<()> {
    let chunk_ids = batch
        .column_by_name("chunk_id")
        .ok_or_else(|| anyhow!("missing chunk_id column"))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| anyhow!("chunk_id not utf8"))?;
    let document_ids = batch
        .column_by_name("document_id")
        .ok_or_else(|| anyhow!("missing document_id column"))?
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| anyhow!("document_id not utf8"))?;
    let distances = batch
        .column_by_name("_distance")
        .ok_or_else(|| anyhow!("missing _distance column"))?
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| anyhow!("_distance not f32"))?;
    for i in 0..batch.num_rows() {
        hits.push(VectorHit {
            chunk_id: chunk_ids.value(i).to_string(),
            document_id: document_ids.value(i).to_string(),
            distance: distances.value(i),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn upsert_and_query_roundtrip() {
        let dir = TempDir::new().unwrap();
        let store = VectorStore::open(dir.path()).unwrap();
        let rows = vec![
            VectorRow {
                chunk_id: "c1".to_string(),
                document_id: "d1".to_string(),
                vector: vec![1.0, 0.0, 0.0],
            },
            VectorRow {
                chunk_id: "c2".to_string(),
                document_id: "d1".to_string(),
                vector: vec![0.0, 1.0, 0.0],
            },
            VectorRow {
                chunk_id: "c3".to_string(),
                document_id: "d2".to_string(),
                vector: vec![0.9, 0.1, 0.0],
            },
        ];
        store.upsert("local__local_hash_v1", 3, &rows).unwrap();
        let hits = store.query("local__local_hash_v1", &[1.0, 0.0, 0.0], 2).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].chunk_id, "c1");

        store.delete_document("d1").unwrap();
        let hits = store.query("local__local_hash_v1", &[1.0, 0.0, 0.0], 5).unwrap();
        assert!(hits.iter().all(|h| h.document_id != "d1"));
    }
}
