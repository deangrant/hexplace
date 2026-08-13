# Architecture

Hexplace splits responsibilities across three crates and columnar on-disk indexes.

```text
OSM PBF ──► import ──► places/{coords,meta,strings}.bin
                   ├──► text/ (Tantivy)
                   └──► spatial/{fine,coarse}/ CSR H3

CLI / HTTP ──► GeocodeService ──► traits
                                 ├── PlaceStore (+ coord/importance)
                                 ├── TextSearcher
                                 └── SpatialSearcher
```

## SOLID boundaries

- **SRP:** import, storage, text search, spatial search, and HTTP each live in
  focused modules.
- **DIP / ISP:** `hexplace-core` owns narrow traits (`PlaceStore`,
  `TextSearcher`, `SpatialSearcher`, `Geocoder`). The engine implements them;
  the binary wires them at startup.
- **OCP:** new index backends can implement the same traits without rewriting
  HTTP handlers. A `ShardRouter` seam allows multi-shard serving later.

Prefer static dispatch inside the engine. The HTTP layer holds an `Arc<Engine>`
as shared state.

## Import pipeline

1. Stream the PBF with `osmpbf`.
2. Collect node coordinates in a `SparseNodeStore` (sorted id array) for way
   centroids on regional extracts. A `FlatNodeStore` impl exists for
   planet-capable imports but is not used by the default extract path.
3. Keep searchable objects: named places, address nodes, named highways, and
   common POI tags.
4. Assign dense `place_id` values and write columnar place files under
   `places/` (`coords.bin`, `meta.bin`, `strings.bin`).
5. Build a Tantivy document per place from names and address fields.
6. Index each place into a single fine H3 cell (res 10) and its coarse cell
   (res 6). Ring expansion happens at query time, not index time.
7. Write `manifest.json` (schema v2) with resolutions and format markers.

## Query paths

### Forward (`/v1/geocode`)

Normalize tokens → Tantivy query (up to `SearchQuery::MAX_LIMIT`
candidates) → hydrate places from columnar store → combine text score
with importance → truncate to the request `limit`.

### Reverse (`/v1/reverse`)

Convert lat/lon to a fine H3 cell → CSR postings slice → ring-expand if
under-populated → coarse fallback if still empty or under the request
`limit` → rank by distance using `coords.bin` only → hydrate string
fields for the `limit` winners.

### Batch (`/v1/batch`)

Process mixed geocode/reverse items in parallel (`std::thread::scope`) against
warm indexes. Up to 100,000 items per request.

### Bulk reverse (`/v1/reverse/bulk`)

Accept JSON `{"points":[[lat,lon],...]}` or packed `f32` pairs. Return NDJSON
or packed binary `(u8 present, u64 place_id, f32 score)` based on `Accept`
(`present == 0` for miss/error). Parallelized the same way as batch.

## Planet scale-up (future)

Extracts use the sparse node store. For planet scale:

- Switch import to `FlatNodeStore` (`8 bytes × max_node_id`, ~100 GB).
- Partition place / Tantivy / CSR files by coarse H3 cell or country.
- Route queries via `ShardRouter` (scaffold already present).

Those changes should not require redesigning the HTTP API.

## Out of scope for this release

- Actually importing a planet file
- Live minutely / diff replication
- Full administrative polygon point-in-polygon hierarchy
- House-number interpolation along ways
- Multilingual importance models beyond tag heuristics
