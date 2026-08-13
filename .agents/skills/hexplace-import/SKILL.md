---
name: hexplace-import
description: >-
  Guide Hexplace PBF import, staging publish, data-dir locks, schema bumps, and
  test fixtures. Use when changing import, indexes, manifest/schema, data-dir
  layout, or writing import/open tests.
trigger: >-
  import, PBF, osmpbf, staging, flock, data-dir, SCHEMA_VERSION, manifest,
  SparseNodeStore, FlatNodeStore, places.bin, testdata, tiny.osm.pbf,
  import_places, publish
---

# Hexplace import and indexes

See setup/ops in [README.md](../../../README.md) and
[ARCHITECTURE.md](../../docs/ARCHITECTURE.md). Use with
`hexplace-architecture` when changing crate placement.

## Pipeline (default extract path)

1. Stream PBF with `osmpbf` (two passes: nodes → coords; nodes/ways → places).
2. `SparseNodeStore` for way centroids (default). `FlatNodeStore` exists for
   planet-scale but is **not** the default extract path — do not switch casually.
3. Searchable objects from **nodes and ways only** (named places / address
   housenumber+street). **Relations are skipped.**
4. Dense `place_id`s → columnar `places/{coords,meta,strings}.bin`.
5. Tantivy docs under `text/`; H3 fine res 10 + coarse res 6 under `spatial/`.
6. Write `manifest.json` (schema v2).

Empty PBF / empty place list → import **error** (not an empty live index).

## Staging, lock, leftovers

- Import writes `.{dirname}.staging-{pid}-{nanos}` beside the target, then
  rename-publishes. Failures call discard; cleanup may leave `.staging-` /
  `.obsolete-` orphans — **not** live data.
- Serve holds a **shared** flock on `data/.lock`; publish needs **exclusive**.
  Stop serve before re-import into the same `--data-dir`, then restart serve.
- Status exposes `source_hash` (SHA-256 of PBF bytes), not the local path.

## Schema bumps

When on-disk format changes incompatibly, bump `SCHEMA_VERSION` and/or format
markers and ensure open/serve reject old directories. Do not silently read v1
layouts as v2.

## Tests and fixtures

- Prefer `import_places(...)` + `tempfile::tempdir` for unit/integration tests.
- Use checked-in [`testdata/tiny.osm.pbf`](../../../testdata/tiny.osm.pbf) only
  for PBF-path coverage (Monaco extract). Assert if missing.
- Do not commit large `.osm.pbf` files outside `testdata/` (gitignored).
