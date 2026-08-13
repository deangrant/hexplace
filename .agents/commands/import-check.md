# Import check

Load skill `.agents/skills/hexplace-import` (and `hexplace-architecture` if
crate placement is unclear).

Review the current change or question for import/index mistakes:

- Staging publish and exclusive lock vs serve shared flock
- Schema / format marker bumps when on-disk layout changes
- Relations still skipped; default path stays on `SparseNodeStore`
- Tests prefer `import_places` + tempfile; PBF only via `testdata/tiny.osm.pbf`
- No treating `.staging-*` / `.obsolete-*` as live data
- No committing large PBFs outside `testdata/`

Report findings and fix only what the user asked to change.
