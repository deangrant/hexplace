//! Engine open and reimport lifecycle integration tests.

#![expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]

mod common;

use common::{sample_places, TestResult};
use hexplace_engine::import::import_places;
use hexplace_engine::{Engine, EngineConfig, Manifest};

#[test]
fn open_rejects_truncated_coords_column() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let coords = dir.path().join("places/coords.bin");
    let bytes = std::fs::read(&coords)?;
    assert!(
        bytes.len() > 16,
        "coords column must contain more than a header"
    );
    std::fs::write(&coords, &bytes[..16])?;
    let Err(err) = Engine::open(EngineConfig::new(dir.path())) else {
        return Err("truncated coords open must fail".into());
    };
    assert!(
        err.to_string().to_lowercase().contains("truncat")
            || err.to_string().contains("place columns"),
        "truncated coords open must cite truncation or place-column damage"
    );
    Ok(())
}

#[test]
fn open_rejects_missing_manifest() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    std::fs::remove_file(dir.path().join("manifest.json"))?;
    let Err(err) = Engine::open(EngineConfig::new(dir.path())) else {
        return Err("missing manifest open must fail".into());
    };
    assert!(
        err.to_string().contains("manifest") || err.to_string().contains("failed to read"),
        "missing manifest open must cite the manifest path or read failure"
    );
    Ok(())
}

#[test]
fn open_rejects_manifest_place_count_mismatch() -> TestResult {
    let dir = tempfile::tempdir()?;
    import_places(sample_places(), dir.path())?;
    let manifest_path = dir.path().join("manifest.json");
    let mut manifest = Manifest::load(&manifest_path)?;
    manifest.place_count += 99;
    manifest.save(&manifest_path)?;
    let Err(err) = Engine::open(EngineConfig::new(dir.path())) else {
        return Err("place_count mismatch open must fail".into());
    };
    assert!(
        err.to_string().contains("place_count"),
        "manifest place_count mismatch must name the place_count field"
    );
    Ok(())
}

#[test]
fn reimport_replaces_live_tree_atomically() -> TestResult {
    let dir = tempfile::tempdir()?;
    let data = dir.path().join("data");
    import_places(sample_places(), &data)?;
    let first = Engine::open(EngineConfig::new(&data))?;
    assert_eq!(
        first.manifest().place_count,
        3,
        "initial import must index all sample places"
    );
    drop(first);

    let mut fewer = sample_places();
    fewer.truncate(1);
    import_places(fewer, &data)?;
    let second = Engine::open(EngineConfig::new(&data))?;
    assert_eq!(
        second.manifest().place_count,
        1,
        "reimport must replace the live tree with the new place count"
    );

    let Some(parent) = data.parent() else {
        return Err("temp data path must have a parent".into());
    };
    let Some(name) = data.file_name().and_then(|s| s.to_str()) else {
        return Err("temp data path must have a utf-8 file name".into());
    };
    let staging_prefix = format!(".{name}.staging-");
    let obsolete_prefix = format!(".{name}.obsolete-");
    for entry in std::fs::read_dir(parent)? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.starts_with(&staging_prefix) && !name.starts_with(&obsolete_prefix),
            "successful publish must leave no staging or obsolete siblings"
        );
    }
    Ok(())
}

#[test]
fn reimport_rejected_while_engine_holds_lock() -> TestResult {
    let dir = tempfile::tempdir()?;
    let data = dir.path().join("data");
    import_places(sample_places(), &data)?;
    let engine = Engine::open(EngineConfig::new(&data))?;
    assert_eq!(
        engine.manifest().place_count,
        3,
        "locked engine must still expose the imported place count"
    );

    let Err(err) = import_places(sample_places(), &data) else {
        return Err("reimport while locked must fail".into());
    };
    let msg = err.to_string();
    assert!(
        msg.contains("data directory in use"),
        "locked reimport must report that the data directory is in use"
    );
    assert_eq!(
        engine.manifest().place_count,
        3,
        "failed reimport must leave the live tree unchanged"
    );
    assert!(
        data.join("manifest.json").is_file(),
        "failed reimport must leave the live manifest in place"
    );
    Ok(())
}
