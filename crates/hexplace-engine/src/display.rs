//! Display name formatting from place fields.

use hexplace_core::{AddressParts, Place};

/// Builds a human-readable display name from name and address parts.
pub fn format_display_name(name: Option<&str>, address: &AddressParts) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(n) = name {
        if !n.is_empty() {
            parts.push(n);
        }
    }
    if let Some(h) = address.house_number.as_deref() {
        parts.push(h);
    }
    if let Some(r) = address.road.as_deref() {
        parts.push(r);
    }
    if let Some(c) = address.city.as_deref() {
        parts.push(c);
    }
    if let Some(p) = address.postcode.as_deref() {
        parts.push(p);
    }
    if let Some(c) = address.country.as_deref() {
        parts.push(c);
    }
    if parts.is_empty() {
        "Unknown place".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Ensures `place.display_name` is populated.
pub fn ensure_display_name(place: &mut Place) {
    if place.display_name.trim().is_empty() {
        place.display_name = format_display_name(place.name.as_deref(), &place.address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_address_parts() {
        let address = AddressParts {
            house_number: Some("10".into()),
            road: Some("Main St".into()),
            city: Some("Testville".into()),
            postcode: Some("12345".into()),
            country: Some("Testland".into()),
            country_code: Some("tl".into()),
        };
        assert_eq!(
            format_display_name(Some("Cafe"), &address),
            "Cafe, 10, Main St, Testville, 12345, Testland"
        );
    }
}
