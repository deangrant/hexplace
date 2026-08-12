//! Tantivy-backed forward text search.

use std::fs;
use std::path::Path;

use geofind_core::{CoreError, Place, PlaceId, SearchQuery, TextSearcher};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STORED, STRING,
};
use tantivy::{doc, Index, IndexReader, ReloadPolicy, TantivyDocument};

use crate::tokenize::normalize_query;

/// Builds a Tantivy index for the given places.
pub fn build_index(places: &[Place], dir: &Path) -> Result<(), CoreError> {
    if dir.exists() {
        fs::remove_dir_all(dir)
            .map_err(|e| CoreError::index(format!("failed to clear text index: {e}")))?;
    }
    fs::create_dir_all(dir)?;

    let (schema, fields) = build_schema();
    let index = Index::create_in_dir(dir, schema)
        .map_err(|e| CoreError::index(format!("create text index: {e}")))?;
    let mut writer = index
        .writer(50_000_000)
        .map_err(|e| CoreError::index(format!("text writer: {e}")))?;

    for place in places {
        let search_text = place_search_text(place);
        writer
            .add_document(doc!(
                fields.place_id => place.place_id.to_string(),
                fields.text => search_text,
                fields.category => place.category.as_str(),
            ))
            .map_err(|e| CoreError::index(format!("add document: {e}")))?;
    }
    writer
        .commit()
        .map_err(|e| CoreError::index(format!("commit text index: {e}")))?;
    Ok(())
}

struct Fields {
    place_id: Field,
    text: Field,
    category: Field,
}

fn build_schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let place_id = builder.add_text_field("place_id", STRING | STORED);
    let text = builder.add_text_field("text", text_opts);
    let category = builder.add_text_field("category", STRING | STORED);
    let schema = builder.build();
    (
        schema,
        Fields {
            place_id,
            text,
            category,
        },
    )
}

fn place_search_text(place: &Place) -> String {
    let mut parts = Vec::new();
    if let Some(name) = &place.name {
        parts.push(name.clone());
    }
    parts.push(place.display_name.clone());
    if let Some(h) = &place.address.house_number {
        parts.push(h.clone());
    }
    if let Some(r) = &place.address.road {
        parts.push(r.clone());
    }
    if let Some(c) = &place.address.city {
        parts.push(c.clone());
    }
    if let Some(p) = &place.address.postcode {
        parts.push(p.clone());
    }
    parts.push(place.category.clone());
    parts.push(place.type_name.clone());
    normalize_query(&parts.join(" "))
}

/// Opened Tantivy text searcher.
pub struct TantivySearcher {
    reader: IndexReader,
    query_parser: QueryParser,
    place_id_field: Field,
}

impl TantivySearcher {
    /// Opens a text index from disk.
    pub fn open(dir: &Path) -> Result<Self, CoreError> {
        let index = Index::open_in_dir(dir)
            .map_err(|e| CoreError::index(format!("open text index: {e}")))?;
        let schema = index.schema();
        let place_id_field = schema
            .get_field("place_id")
            .map_err(|e| CoreError::index(format!("missing place_id field: {e}")))?;
        let text_field = schema
            .get_field("text")
            .map_err(|e| CoreError::index(format!("missing text field: {e}")))?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e| CoreError::index(format!("text reader: {e}")))?;
        let query_parser = QueryParser::for_index(&index, vec![text_field]);
        Ok(Self {
            reader,
            query_parser,
            place_id_field,
        })
    }
}

impl TextSearcher for TantivySearcher {
    fn search(&self, query: &SearchQuery) -> Result<Vec<(PlaceId, f32)>, CoreError> {
        let normalized = normalize_query(&query.q);
        if normalized.is_empty() {
            return Err(CoreError::invalid("query has no searchable tokens"));
        }
        let tq = self
            .query_parser
            .parse_query(&normalized)
            .map_err(|e| CoreError::index(format!("parse query: {e}")))?;
        let searcher = self.reader.searcher();
        let top = searcher
            .search(&tq, &TopDocs::with_limit(query.limit))
            .map_err(|e| CoreError::index(format!("search failed: {e}")))?;

        let mut out = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let doc: TantivyDocument = searcher
                .doc(addr)
                .map_err(|e| CoreError::index(format!("fetch doc: {e}")))?;
            let id_str = doc
                .get_first(self.place_id_field)
                .and_then(|v| v.as_str())
                .ok_or_else(|| CoreError::index("document missing place_id"))?;
            let id: PlaceId = id_str
                .parse()
                .map_err(|e| CoreError::index(format!("bad place_id: {e}")))?;
            out.push((id, score));
        }
        Ok(out)
    }
}
