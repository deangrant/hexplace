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
  places.bin
  text/           # Tantivy segments
  spatial/h3.bin  # H3 cell → place_id postings
```

`geofind serve` refuses to start if `manifest.json` is missing or the schema
version is unsupported.

## Environment variables

| Variable | Default | Meaning |
| --- | --- | --- |
| `GEOFIND_DATA` | `./data` | Data directory |
| `GEOFIND_BIND` | `127.0.0.1:8080` | HTTP listen address |
| `RUST_LOG` | `info` | Tracing filter |

## Importing extracts

Use regional extracts while developing. Full planet import is supported by the
same pipeline but needs much more RAM, disk, and time.

```bash
geofind import --pbf path/to/region.osm.pbf --data-dir ./data
```

Rough guidance:

| Extract size | Typical RAM during import | Notes |
| --- | --- | --- |
| City / small country | 1–4 GiB | Comfortable on a laptop |
| Large country | 8–32 GiB | SSD strongly recommended |
| Planet | 64+ GiB | Plan for sharded indexes later |

Import currently caches node coordinates in memory so way centroids can be
computed. That is fine for regional extracts; planet-scale imports will need
the partitioned scale-up path described in `architecture.md`.

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
