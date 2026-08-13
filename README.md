# Hexplace

Hexplace is a high-performance OpenStreetMap geocoding service written in Rust.
It imports regional OSM extracts into local memory-mapped indexes and serves
forward search, reverse geocoding, and bulk batch queries over a simple HTTP
API and CLI.

## Why this design

Hexplace keeps indexes **in-process** (mmap place store + Tantivy text index +
H3 spatial postings). That avoids a separate database round-trip on every
query and is a better fit for high-throughput bulk geocoding on a single
machine or server.

## Requirements

- Rust toolchain via [rustup](https://rustup.rs/) (see `rust-toolchain.toml`)
- Disk space for your OSM extract and derived indexes
- An OSM `.osm.pbf` extract (for example from
  [Geofabrik](https://download.geofabrik.de/))

There is no Python virtualenv. The isolated “environment” for Hexplace is:

1. The pinned Rust toolchain
2. A dedicated data directory (`./data` by default, or `HEXPLACE_DATA`)

## Quick start

```bash
# Install / update the pinned toolchain
rustup show

# Build
cargo build --release

# Download a regional extract (example: Monaco)
mkdir -p extracts
curl -L -o extracts/monaco.osm.pbf \
  https://download.geofabrik.de/europe/monaco-latest.osm.pbf

# Import
./target/release/hexplace import \
  --pbf extracts/monaco.osm.pbf \
  --data-dir ./data

# Serve
./target/release/hexplace serve --data-dir ./data --bind 127.0.0.1:8080
```

### CLI queries

```bash
hexplace search --data-dir ./data "Monaco"
hexplace reverse --data-dir ./data --lat 43.7384 --lon 7.4246
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

### Reverse bench

```bash
hexplace bench --data-dir ./data --lat 43.7384 --lon 7.4246 --count 10000
```

## Workspace layout

| Crate | Role |
| --- | --- |
| `hexplace-core` | Domain types and narrow traits |
| `hexplace-engine` | Import, mmap store, Tantivy, H3, geocoder |
| `hexplace` | CLI + Axum HTTP composition root |

## Tests

```bash
cargo test
```

A small Monaco extract is checked in under `testdata/tiny.osm.pbf` for
integration coverage.

## Documentation

- [Setup](docs/setup.md) — data directory, env vars, hardware notes
- [Architecture](docs/architecture.md) — import pipeline and scale-up path

## License

Licensed under either of

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.

OpenStreetMap data is © OpenStreetMap contributors and is available under the
[Open Database License (ODbL)](https://opendatacommons.org/licenses/odbl/).
