//! 资产 embedding 向量存储与相似度检索。

use crate::{Store, StoreError};
use rusqlite::params;

/// 资产 embedding 向量存储记录。
pub struct AssetEmbedding {
    pub id: String,
    pub asset_type: String,
    pub asset_id: String,
    pub chunk_index: i32,
    pub chunk_text: String,
    /// `Vec<f32>` 编码为小端字节序列（4 字节/元素）。
    pub embedding: Vec<f32>,
    pub model: String,
    pub updated_at: String,
}

/// 相似度检索结果。
pub struct AssetEmbeddingSearchResult {
    pub id: String,
    pub asset_type: String,
    pub asset_id: String,
    pub chunk_index: i32,
    pub chunk_text: String,
    pub score: f32,
}

/// 将 `Vec<f32>` 编码为小端字节 blob。
fn encode_embedding(vec: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vec.len() * 4);
    for &val in vec {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// 从小端字节 blob 解码 `Vec<f32>`。
fn decode_embedding(blob: &[u8]) -> Result<Vec<f32>, StoreError> {
    if !blob.len().is_multiple_of(4) {
        return Err(StoreError::Rusqlite(rusqlite::Error::InvalidParameterName(
            "embedding blob 长度不是 4 的倍数".to_owned(),
        )));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

/// 余弦相似度。
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b + 1e-8)
}

impl Store {
    /// 写入或更新一条 embedding 记录。
    pub fn upsert_asset_embedding(&self, record: &AssetEmbedding) -> Result<(), StoreError> {
        let blob = encode_embedding(&record.embedding);
        self.db().execute(
            "INSERT OR REPLACE INTO asset_embeddings
             (id, asset_type, asset_id, chunk_index, chunk_text, embedding, model, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                record.id,
                record.asset_type,
                record.asset_id,
                record.chunk_index,
                record.chunk_text,
                blob,
                record.model,
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 删除指定资产的所有 embedding。
    pub fn delete_asset_embeddings(
        &self,
        asset_type: &str,
        asset_id: &str,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM asset_embeddings WHERE asset_type = ?1 AND asset_id = ?2",
            params![asset_type, asset_id],
        )?;
        Ok(())
    }

    /// 删除所有 embedding 记录（用于全量重建索引）。
    pub fn delete_all_asset_embeddings(&self) -> Result<(), StoreError> {
        self.db().execute("DELETE FROM asset_embeddings", [])?;
        Ok(())
    }

    /// 基于查询向量检索最相似的 embedding 记录。
    ///
    /// 加载全量 embedding 到内存后在 Rust 侧计算 cosine similarity，
    /// 返回按相似度降序排列的 top-K 结果。
    pub fn search_similar(
        &self,
        query: &[f32],
        asset_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AssetEmbeddingSearchResult>, StoreError> {
        let db = self.db();
        let sql = match asset_type {
            Some(_) => {
                "SELECT id, asset_type, asset_id, chunk_index, chunk_text, embedding FROM asset_embeddings WHERE asset_type = ?1"
            }
            None => {
                "SELECT id, asset_type, asset_id, chunk_index, chunk_text, embedding FROM asset_embeddings"
            }
        };
        let mut stmt = db.prepare(sql)?;

        let mut rows = match asset_type {
            Some(at) => stmt.query(params![at])?,
            None => stmt.query([])?,
        };

        let mut candidates: Vec<AssetEmbeddingSearchResult> = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let at: String = row.get(1)?;
            let aid: String = row.get(2)?;
            let ci: i32 = row.get(3)?;
            let ct: String = row.get(4)?;
            let blob: Vec<u8> = row.get(5)?;

            let embedding = decode_embedding(&blob)?;
            let score = cosine_similarity(query, &embedding);

            candidates.push(AssetEmbeddingSearchResult {
                id,
                asset_type: at,
                asset_id: aid,
                chunk_index: ci,
                chunk_text: ct,
                score,
            });
        }

        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(limit);
        Ok(candidates)
    }

    /// 统计 embedding 记录数（用于判断是否需要重建索引）。
    pub fn count_asset_embeddings(&self) -> Result<u64, StoreError> {
        let count: i64 =
            self.db()
                .query_row("SELECT COUNT(*) FROM asset_embeddings", [], |row| {
                    row.get(0)
                })?;
        #[allow(clippy::cast_sign_loss)]
        Ok(count as u64)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let original = vec![1.0_f32, -0.5, 0.0, 4.2];
        let blob = encode_embedding(&original);
        assert_eq!(blob.len(), 16);
        let decoded = decode_embedding(&blob).unwrap();
        assert_eq!(decoded.len(), 4);
        for (a, b) in original.iter().zip(decoded.iter()) {
            assert!(f32::abs(a - b) < 1e-6);
        }
    }

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0_f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!(f32::abs(sim - 1.0) < 1e-5);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(f32::abs(sim) < 1e-5);
    }

    #[test]
    fn upsert_and_search() {
        let store = Store::open_in_memory().unwrap();

        let record = AssetEmbedding {
            id: "emb-1".to_owned(),
            asset_type: "device".to_owned(),
            asset_id: "dev-1".to_owned(),
            chunk_index: 0,
            chunk_text: "温度传感器".to_owned(),
            embedding: vec![1.0_f32, 0.0, 0.0],
            model: "test".to_owned(),
            updated_at: "2026-01-01T00:00:00Z".to_owned(),
        };
        store.upsert_asset_embedding(&record).unwrap();

        let query = vec![0.9_f32, 0.1, 0.0];
        let results = store.search_similar(&query, None, 5).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].asset_id, "dev-1");
        assert!(results[0].score > 0.9);

        let filtered = store.search_similar(&query, Some("capability"), 5).unwrap();
        assert!(filtered.is_empty());
    }
}
