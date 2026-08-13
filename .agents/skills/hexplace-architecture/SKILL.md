---
name: hexplace-architecture
description: >-
  Explain Hexplace crate layout, on-disk indexes, query paths, and product
  boundaries. Use when designing or refactoring modules, deciding where code
  belongs, changing HTTP/CLI APIs, or asking how geocode/reverse/batch work.
trigger: >-
  architecture, crate boundaries, where does X live, Engine, PlaceStore,
  Tantivy, H3 CSR, geocode, reverse, batch, reverse-bulk, schema v2, out of scope
---

# Hexplace architecture

Condensed from [ARCHITECTURE.md](../../docs/ARCHITECTURE.md) and setup/ops
sections in [README.md](../../../README.md). Prefer those docs for full detail.
Also read root [AGENTS.md](../../../AGENTS.md).

## Crate map

```text
OSM PBF ──► import ──► places/{coords,meta,strings}.bin
                   ├──► text/ (Tantivy)
                   └──► spatial/{fine,coarse}/ CSR H3

CLI / HTTP ──► GeocodeService / Engine ──► traits
                                         ├── PlaceStore
                                         ├── TextSearcher
                                         └── SpatialSearcher
```

- `hexplace-core`: traits + domain types
- `hexplace-engine`: indexes + `Engine` (composition root, static dispatch)
- `hexplace`: CLI + Axum; `Arc<Engine>` as shared state

## On-disk layout (schema v2)

- `manifest.json` — schema version and format markers; serve refuses unsupported
- `places/` — columnar bins (not legacy `places.bin`)
- `text/` — Tantivy segments
- `spatial/fine` (H3 res 10), `spatial/coarse` (H3 res 6); ring expand at query time

## Query paths

| Path | Behavior |
| --- | --- |
| Forward `/v1/geocode` | Tantivy → hydrate → text score + importance → `limit` |
| Reverse `/v1/reverse` | Fine H3 → CSR → ring/coarse fallback → distance rank |
| Batch `/v1/batch` | Mixed ops in parallel; up to 100,000 items |
| Bulk reverse `/v1/reverse/bulk` | JSON or packed `f32` pairs; NDJSON or 13-byte LE records |

## File-size hotspots

Style skill caps `.rs` files at ~500 lines. Split before growing past ~450:

- `crates/hexplace-engine/src/service.rs`
- Large engine integration tests (e.g. `geocode_flow.rs`)

## Out of scope

Do not implement or scaffold: planet import/sharding, minutely diffs, OSM
relations / admin PIP, house-number interpolation, Nominatim display names,
learned importance. Planet notes in architecture.md are future guidance only.
