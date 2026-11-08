#![expect(
    clippy::panic_in_result_fn,
    reason = "Result tests assert contracts after fallible setup via ?"
)]
use hexplace_core::{AddressParts, OsmType};

use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn sample_place(name: &str) -> Place {
    Place {
        place_id: 0,
        osm_type: OsmType::Node,
        osm_id: 1,
        lat: 48.0,
        lon: 2.0,
        name: Some(name.to_owned()),
        display_name: name.to_owned(),
        category: "place".to_owned(),
        type_name: "city".to_owned(),
        address: AddressParts::default(),
        importance: 0.5,
    }
}

#[test]
fn round_trip_places() -> TestResult {
    let dir = tempfile::tempdir()?;
    let paths = PlacePaths::new(dir.path().join("places"));
    let mut w = PlaceStoreWriter::new();
    w.push(sample_place("Alpha"));
    w.push(sample_place("Beta"));
    w.write_to(&paths)?;

    let store = MmapPlaceStore::open(&paths)?;
    assert_eq!(store.len(), 2, "writer must persist both places");
    assert_eq!(
        store.get(0)?.name.as_deref(),
        Some("Alpha"),
        "first place name must round-trip"
    );
    assert_eq!(
        store.get(1)?.name.as_deref(),
        Some("Beta"),
        "second place name must round-trip"
    );
    let (lat, lon) = store.coord(0)?;
    assert!(
        (lat - 48.0).abs() < 1e-6,
        "latitude must round-trip within 1e-7 encoding"
    );
    assert!(
        (lon - 2.0).abs() < 1e-6,
        "longitude must round-trip within 1e-7 encoding"
    );
    let importance = store.importance(0)?;
    assert!(
        (importance - 0.5).abs() < f32::EPSILON,
        "importance must round-trip exactly for a finite f32 literal"
    );
    Ok(())
}

#[test]
fn encode_rejects_embedded_nul() -> TestResult {
    let mut place = sample_place("ok");
    place.name = Some("bad\0name".into());
    let Err(err) = encode_strings(&place) else {
        return Err("embedded NUL in name must fail encode".into());
    };
    assert!(
        err.to_string().contains("NUL"),
        "NUL rejection must mention NUL"
    );

    place.name = Some("ok".into());
    place.display_name = "bad\0display".into();
    let Err(err) = encode_strings(&place) else {
        return Err("embedded NUL in display_name must fail encode".into());
    };
    assert!(
        err.to_string().contains("NUL"),
        "NUL rejection must mention NUL"
    );
    Ok(())
}

#[test]
fn split_cstrings_rejects_malformed_blobs() -> TestResult {
    let missing_final = b"a\0b\0c\0d\0e";
    let Err(err) = split_cstrings(missing_final) else {
        return Err("blob without final NUL must fail".into());
    };
    assert!(
        err.to_string().contains("missing final NUL"),
        "malformed blob error must name the missing final NUL"
    );

    let too_few = b"a\0b\0c\0";
    let parts = split_cstrings(too_few)?;
    assert_ne!(
        parts.len(),
        5,
        "too-few fields must not look like a full place blob"
    );

    let exact = b"a\0b\0c\0d\0{}\0";
    let parts = split_cstrings(exact)?;
    assert_eq!(parts.len(), 5, "exact five-field blob must split into five");
    assert_eq!(parts[4], "{}", "address JSON field must be preserved");
    Ok(())
}
