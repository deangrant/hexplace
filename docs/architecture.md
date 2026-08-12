# Architecture

Geofind splits responsibilities across three crates and three on-disk indexes.

```text
OSM PBF ──► import ──► places.bin (mmap)
                   ├──► text/ (Tantivy)
                   └──► spatial/h3.bin (H3 postings)

CLI / HTTP ──► GeocodeService ──► traits
                                 ├── PlaceStore
                                 ├── TextSearcher
                                 └── SpatialSearcher
```

## SOLID boundaries

- **SRP:** import, storage, text search, spatial search, and HTTP each live in
  focused modules.
- **DIP / ISP:** `geofind-core` owns narrow traits (`PlaceStore`,
  `TextSearcher`, `SpatialSearcher`, `Geocoder`). The engine implements them;
  the binary wires them at startup.
- **OCP:** new index backends can implement the same traits without rewriting
  HTTP handlers.

Prefer static dispatch inside the engine. The HTTP layer holds an `Arc<Engine>`
as shared state.

## Import pipeline

1. Stream the PBF with `osmpbf`.
2. Collect node coordinates (needed for way centroids on regional extracts).
3. Keep searchable objects: named places, address nodes, named highways, and
   common POI tags.
4. Assign dense `place_id` values and write `places.bin` (offset table + JSON
   records).
5. Build a Tantivy document per place from names and address fields.
6. Map each place into H3 cells (default resolution 9) plus a 1-ring for recall.
7. Write `manifest.json` with schema version, source fingerprint, and counts.

## Query paths

### Forward (`/v1/geocode`)

Normalize tokens → Tantivy query → hydrate places from mmap → combine text
score with tag-based importance → truncate to `limit`.

### Reverse (`/v1/reverse`)

Convert lat/lon to an H3 cell → gather candidates from postings (expand ring if
needed) → rank by haversine distance and importance → truncate to `limit`.

### Batch (`/v1/batch`)

Process mixed geocode/reverse items in one request against warm indexes. This
is the primary bulk path: no per-row process startup and no remote DB hop.

## Planet scale-up (future)

v1 targets regional extracts with a single data directory. For planet scale,
keep the same traits and shard by geography:

- Partition `places.bin` / Tantivy / H3 files by H3 parent or country code.
- Route queries to the relevant shard(s) in the `Geocoder` implementation.
- Stream node coordinates during import (or use a disk-backed coord store)
  instead of a single in-memory map.

Those changes should not require redesigning the HTTP API.

## Out of scope for v1

- Live minutely / diff replication
- Full administrative polygon point-in-polygon hierarchy
- House-number interpolation along ways
- Multilingual importance models beyond tag heuristics
