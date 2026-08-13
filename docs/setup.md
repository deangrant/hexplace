# Setup

## Toolchain

Geofind pins a Rust toolchain in `rust-toolchain.toml`. Install rustup, then
from the repository root:

```bash
rustup show
cargo build --release
```

Optional quality checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

## Data directory

Default data directory is `./data`. Override with `--data-dir` or the
`GEOFIND_DATA` environment variable.

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

`geofind serve` refuses to start if `manifest.json` is missing or the schema
version / format markers are unsupported.

## Environment variables

| Variable | Default | Meaning |
| --- | --- | --- |
| `GEOFIND_DATA` | `./data` | Data directory |
| `GEOFIND_BIND` | `127.0.0.1:8080` | HTTP listen address |
| `RUST_LOG` | `info` | Tracing filter |

## Importing extracts

Use regional extracts while developing.

```bash
geofind import --pbf path/to/region.osm.pbf --data-dir ./data
```

Rough guidance:

| Extract size | Typical RAM during import | Notes |
| --- | --- | --- |
| City / small country | 1–4 GiB | Comfortable on a laptop |
| Large country | 8–32 GiB | SSD strongly recommended |

Import uses a sparse sorted node-id store for way centroids (scales with
stored nodes, not the global OSM id range).

## Running as a service

```bash
export GEOFIND_DATA=/var/lib/geofind
export GEOFIND_BIND=0.0.0.0:8080
geofind serve
```

Put a reverse proxy in front for TLS. Use `/v1/health` for liveness and
`/v1/status` for readiness (index loaded and manifest readable).

## Batch CLI

JSONL input, one object per line:

```json
{"op":"geocode","id":"1","q":"Monaco","limit":5}
{"op":"reverse","id":"2","lat":43.7384,"lon":7.4246,"limit":1}
```

```bash
geofind batch --data-dir ./data --file queries.jsonl
```

## Bulk reverse HTTP

```bash
curl -s -H 'content-type: application/json' \
  -H 'accept: application/x-ndjson' \
  --data '{"points":[[43.7384,7.4246],[43.7310,7.4210]]}' \
  http://127.0.0.1:8080/v1/reverse/bulk
```

## Reverse bench

```bash
geofind bench --data-dir ./data --lat 43.7384 --lon 7.4246 --count 10000
```
