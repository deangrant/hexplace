# Hexplace

Hexplace is a Rust OpenStreetMap geocoder for **regional extracts**. It imports
an `.osm.pbf` into local memory-mapped indexes (columnar places, Tantivy text,
H3 spatial postings) and serves forward search, reverse geocoding, mixed batch,
and bulk reverse over a CLI and HTTP API—without a separate database process.

It is **not** a Nominatim replacement; see [Known limitations](#known-limitations).

## Why this design

Indexes stay **in-process** (mmap place store + Tantivy + H3 CSR). That avoids a
database round-trip per query and fits high-throughput bulk geocoding on one
machine.

## Requirements

- [rustup](https://rustup.rs/) (see `rust-toolchain.toml`; workspace MSRV 1.81)
- Disk for the extract and derived indexes
- A regional OSM `.osm.pbf` (for example from
  [Geofabrik](https://download.geofabrik.de/))

No Python environment. The runtime surface is the pinned Rust toolchain plus a
data directory (`./data` by default, or `HEXPLACE_DATA`).

## Quick start

```bash
rustup show
cargo build --release

mkdir -p extracts
curl -L -o extracts/monaco.osm.pbf \
  https://download.geofabrik.de/europe/monaco-latest.osm.pbf

./target/release/hexplace import \
  --pbf extracts/monaco.osm.pbf \
  --data-dir ./data

./target/release/hexplace serve --data-dir ./data --bind 127.0.0.1:8080
```

### CLI queries

```bash
./target/release/hexplace search --data-dir ./data "Monaco"
./target/release/hexplace reverse --data-dir ./data --lat 43.7384 --lon 7.4246
```

### HTTP API

```bash
curl 'http://127.0.0.1:8080/v1/health'
curl 'http://127.0.0.1:8080/v1/status'
curl 'http://127.0.0.1:8080/v1/geocode?q=Monaco&limit=5'
curl 'http://127.0.0.1:8080/v1/reverse?lat=43.7384&lon=7.4246&limit=1'
curl -X POST 'http://127.0.0.1:8080/v1/batch' \
  -H 'content-type: application/json' \
  -d '{"items":[{"op":"geocode","q":"Monaco"},{"op":"reverse","lat":43.7384,"lon":7.4246}]}'
curl -X POST 'http://127.0.0.1:8080/v1/reverse/bulk' \
  -H 'content-type: application/json' \
  -H 'accept: application/x-ndjson' \
  -d '{"points":[[43.7384,7.4246],[43.7310,7.4210]]}'
```

## Workspace layout

| Crate | Role |
| --- | --- |
| `hexplace-core` | Domain types and narrow traits (`PlaceStore`, `TextSearcher`, `SpatialSearcher`, `Geocoder`) |
| `hexplace-engine` | Import, columnar store, Tantivy, H3, `Engine` composition root |
| `hexplace` | CLI + Axum HTTP (holds `Arc<Engine>`) |

Dependency direction: `hexplace` → `hexplace-engine` → `hexplace-core`.

## Setup

### Toolchain and quality checks

```bash
rustup show
cargo build --release

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Environment variables

| Variable | Default | Meaning |
| --- | --- | --- |
| `HEXPLACE_DATA` | `./data` | Data directory (`--data-dir` overrides) |
| `HEXPLACE_BIND` | `127.0.0.1:8080` | HTTP listen address for `serve` |
| `RUST_LOG` | `info` | Tracing filter |

### Data directory

After import the layout is:

```text
data/
  manifest.json
  places/
    coords.bin      # i32 lat/lon e7
    meta.bin        # osm type, importance, string offsets
    strings.bin     # name / display / category / address
  text/             # Tantivy segments
  spatial/
    fine/           # CSR H3 res 10
    coarse/         # CSR H3 res 6 fallback
```

The place store is the `places/` directory (not a legacy top-level
`places.bin`). `hexplace serve` refuses to start if `manifest.json` is missing
or the schema version / format markers are unsupported (schema v2).

### Importing extracts

Use regional extracts while developing.

```bash
./target/release/hexplace import --pbf path/to/region.osm.pbf --data-dir ./data
```

Import writes a complete staging tree beside the target, then publishes with
directory renames so a failed import cannot leave a torn live index. Stop
`hexplace serve` before re-importing into the same `--data-dir`, then restart
serve to load the new indexes (import refuses to publish while serve holds the
data-directory lock).

| Extract size | Typical RAM during import | Notes |
| --- | --- | --- |
| City / small country | 1–4 GiB | Comfortable on a laptop |
| Large country | 8–32 GiB | SSD strongly recommended |

Import uses a sparse sorted node-id store for way centroids (scales with stored
nodes, not the global OSM id range). Relations are not imported.

### Running as a service

```bash
export HEXPLACE_DATA=/var/lib/hexplace
export HEXPLACE_BIND=0.0.0.0:8080
./target/release/hexplace serve
```

Put a reverse proxy in front for TLS. Use `/v1/health` for liveness and
`/v1/status` for readiness (index loaded and manifest readable). Status returns
schema/place counts, H3 resolutions, format markers, `built_at_unix`, and
`source_hash` (SHA-256 of the imported PBF contents)—not the local import path.

### CLI reference

| Subcommand | Purpose |
| --- | --- |
| `import --pbf <PATH>` | Build indexes into `--data-dir` |
| `serve` | HTTP API (`--bind`) |
| `search <QUERY>` | Forward geocode (`--limit`, default 10, max 50) |
| `reverse --lat --lon` | Reverse geocode (`--limit`, default 1, max 50) |
| `batch --file <PATH\|->` | JSONL batch (stdin with `-`) |
| `bench` | Reverse throughput sample (`--count`) |

Batch JSONL (one object per line):

```json
{"op":"geocode","id":"1","q":"Monaco","limit":5}
{"op":"reverse","id":"2","lat":43.7384,"lon":7.4246,"limit":1}
```

```bash
./target/release/hexplace batch --data-dir ./data --file queries.jsonl
```

```bash
./target/release/hexplace bench --data-dir ./data \
  --lat 43.7384 --lon 7.4246 --count 10000
```

### HTTP routes

| Method | Path | Notes |
| --- | --- | --- |
| `GET` | `/v1/health` | Liveness |
| `GET` | `/v1/status` | Readiness + index metadata |
| `GET` | `/v1/geocode` | `q`, optional `limit` (default 10, max 50) |
| `GET` | `/v1/reverse` | `lat`, `lon`, optional `limit` (default 1, max 50) |
| `POST` | `/v1/batch` | JSON `{ "items": [...] }`; up to 100,000 items; 256 MiB body; 600s timeout |
| `POST` | `/v1/reverse/bulk` | JSON `{"points":[[lat,lon],...]}` or packed LE `f32` pairs |

Bulk reverse with `Accept: application/octet-stream` returns 13-byte
little-endian records: `u8 present`, `u64 place_id`, `f32 score`.
`present == 0` means miss or error (`place_id` and `score` are then zero).
Default accept is NDJSON.

## Tests

```bash
cargo test --workspace
```

A small Monaco extract is checked in under `testdata/tiny.osm.pbf` for
integration coverage.

## Known limitations

- Regional extracts only — no planet import or multi-shard query routing
- No live minutely / diff replication
- Nodes and ways only — no OSM relation import (admin boundaries, multipolygon
  POIs) and no administrative polygon point-in-polygon hierarchy
- No house-number interpolation along ways
- Display names use a fixed comma-separated field order (name, house, road,
  city, postcode, country) — not Nominatim-style or locale-aware labels
- Importance is a coarse OSM tag heuristic — not learned, population-based, or
  multilingual models
- Re-import into a live `--data-dir` requires stopping `serve` first (exclusive
  publish lock while serve holds a shared flock)
- Query caps: geocode/reverse `limit` max 50; batch/bulk up to 100,000 items

See [`.agents/docs/ARCHITECTURE.md`](.agents/docs/ARCHITECTURE.md) for pipeline
detail and future planet scale-up notes.

## Documentation

- [Architecture](.agents/docs/ARCHITECTURE.md) — import pipeline, query paths,
  out of scope, and future planet scale-up notes
- [AGENTS.md](AGENTS.md) — index of agent rules, skills, commands, and hooks

## License

Licensed under the MIT license (`LICENSE`).

OpenStreetMap data is © OpenStreetMap contributors and is available under the
[Open Database License (ODbL)](https://opendatacommons.org/licenses/odbl/).
