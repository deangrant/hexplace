//! Count-capped streaming JSON parsers for batch and reverse-bulk bodies.

use std::fmt;
use std::marker::PhantomData;

use hexplace_core::{BatchItem, BatchRequest, CoreError};
use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

/// Parses `{"points":[[lat,lon],...]}` without retaining more than `max` points.
///
/// # Errors
///
/// Returns an invalid-request error when JSON is malformed, `points` is missing
/// or empty, or the array would exceed `max` elements.
pub fn parse_capped_reverse_bulk_points(
    bytes: &[u8],
    max: usize,
) -> Result<Vec<[f64; 2]>, CoreError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let points = CappedPointsRoot { max }
        .deserialize(&mut de)
        .map_err(|e| CoreError::invalid(format!("invalid bulk JSON: {e}")))?;
    de.end().map_err(|e| CoreError::invalid(format!("invalid bulk JSON: {e}")))?;
    if points.is_empty() {
        return Err(CoreError::invalid("points must not be empty"));
    }
    Ok(points)
}

/// Parses `{"items":[...]}` without retaining more than `max` batch items.
///
/// # Errors
///
/// Returns an invalid-request error when JSON is malformed, `items` is missing
/// or empty, or the array would exceed `max` elements.
pub fn parse_capped_batch_request(bytes: &[u8], max: usize) -> Result<BatchRequest, CoreError> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let items = CappedItemsRoot { max }
        .deserialize(&mut de)
        .map_err(|e| CoreError::invalid(format!("invalid batch JSON: {e}")))?;
    de.end().map_err(|e| CoreError::invalid(format!("invalid batch JSON: {e}")))?;
    if items.is_empty() {
        return Err(CoreError::invalid("batch items must not be empty"));
    }
    Ok(BatchRequest { items })
}

struct CappedPointsRoot {
    max: usize,
}

impl<'de> DeserializeSeed<'de> for CappedPointsRoot {
    type Value = Vec<[f64; 2]>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(CappedFieldVisitor::<[f64; 2]> {
            field: "points",
            max: self.max,
            limit_noun: "points",
            _marker: PhantomData,
        })
    }
}

struct CappedItemsRoot {
    max: usize,
}

impl<'de> DeserializeSeed<'de> for CappedItemsRoot {
    type Value = Vec<BatchItem>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(CappedFieldVisitor::<BatchItem> {
            field: "items",
            max: self.max,
            limit_noun: "items",
            _marker: PhantomData,
        })
    }
}

struct CappedFieldVisitor<T> {
    field: &'static str,
    max: usize,
    limit_noun: &'static str,
    _marker: PhantomData<T>,
}

impl<'de, T> Visitor<'de> for CappedFieldVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a JSON object with an `{}` array", self.field)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values: Option<Vec<T>> = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == self.field {
                if values.is_some() {
                    return Err(de::Error::duplicate_field(self.field));
                }
                values = Some(map.next_value_seed(CappedSeq {
                    max: self.max,
                    limit_noun: self.limit_noun,
                    _marker: PhantomData,
                })?);
            } else {
                map.next_value::<de::IgnoredAny>()?;
            }
        }
        values.ok_or_else(|| de::Error::missing_field(self.field))
    }
}

struct CappedSeq<T> {
    max: usize,
    limit_noun: &'static str,
    _marker: PhantomData<T>,
}

impl<'de, T> DeserializeSeed<'de> for CappedSeq<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(CappedSeqVisitor {
            max: self.max,
            limit_noun: self.limit_noun,
            _marker: PhantomData,
        })
    }
}

struct CappedSeqVisitor<T> {
    max: usize,
    limit_noun: &'static str,
    _marker: PhantomData<T>,
}

impl<'de, T> Visitor<'de> for CappedSeqVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "an array with at most {} elements", self.max)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let hint = seq.size_hint().unwrap_or(0).min(self.max);
        let mut out = Vec::with_capacity(hint);
        while let Some(item) = seq.next_element::<T>()? {
            if out.len() >= self.max {
                // Drop `item` without pushing; `out` stays at `max`.
                return Err(de::Error::custom(format!(
                    "bulk limited to {} {}",
                    self.max, self.limit_noun
                )));
            }
            out.push(item);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::panic_in_result_fn,
        reason = "Result tests assert contracts after fallible setup via ?"
    )]

    use hexplace_engine::Engine;

    use super::*;

    type TestResult = Result<(), String>;

    #[test]
    fn reverse_bulk_points_reject_over_cap_without_keeping_extras() -> TestResult {
        let body = "{\"points\":[[0,0],[0,0],[0,0]]}";
        let Err(err) = parse_capped_reverse_bulk_points(body.as_bytes(), 2) else {
            return Err("over-cap JSON must fail during deserialize".into());
        };
        assert!(
            err.to_string().contains("limited"),
            "error must mention the item limit: {err}"
        );
        // Cap constant stays aligned with the engine policy used by HTTP.
        assert_eq!(Engine::MAX_BATCH_ITEMS, 100_000);
        Ok(())
    }

    #[test]
    fn reverse_bulk_points_accept_at_cap() -> TestResult {
        let body = "{\"points\":[[1.0,2.0],[3.0,4.0]]}";
        let points = parse_capped_reverse_bulk_points(body.as_bytes(), 2)
            .map_err(|e| format!("at-cap JSON must parse: {e}"))?;
        assert_eq!(points, vec![[1.0, 2.0], [3.0, 4.0]]);
        Ok(())
    }

    #[test]
    fn batch_items_reject_over_cap() -> TestResult {
        let mut body = String::from("{\"items\":[");
        for i in 0..3 {
            if i > 0 {
                body.push(',');
            }
            body.push_str(r#"{"op":"geocode","q":"x"}"#);
        }
        body.push_str("]}");
        let Err(err) = parse_capped_batch_request(body.as_bytes(), 2) else {
            return Err("over-cap batch JSON must fail during deserialize".into());
        };
        assert!(
            err.to_string().contains("limited"),
            "error must mention the item limit: {err}"
        );
        Ok(())
    }
}
