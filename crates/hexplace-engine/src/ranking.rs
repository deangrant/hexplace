//! Ranking helpers for forward and reverse results.

use hexplace_core::Place;

/// Combines text relevance with place importance.
#[must_use]
pub fn forward_score(text_score: f32, place: &Place) -> f32 {
    text_score + place.importance
}

/// Haversine distance in meters between two WGS84 points.
#[must_use]
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let to_rad = |d: f64| d.to_radians();
    let dlat = to_rad(lat2 - lat1);
    let dlon = to_rad(lon2 - lon1);
    let sin_dlat = (dlat / 2.0).sin();
    let sin_dlon = (dlon / 2.0).sin();
    let a =
        (to_rad(lat1).cos() * to_rad(lat2).cos()).mul_add(sin_dlon * sin_dlon, sin_dlat * sin_dlat);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

/// Scores a reverse candidate: closer and more important ranks higher.
#[must_use]
pub fn reverse_score(distance_m: f64, importance: f32) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "distance meters compressed into f32 score term"
    )]
    let distance_term = 1.0 / (1.0 + (distance_m as f32 / 50.0));
    importance.mul_add(0.25, distance_term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closer_scores_higher() {
        let near = reverse_score(10.0, 0.1);
        let far = reverse_score(500.0, 0.1);
        assert!(
            near > far,
            "closer reverse candidates must outrank farther ones at equal importance"
        );
    }

    #[test]
    fn haversine_zero_for_same_point() {
        assert!(
            haversine_m(1.0, 2.0, 1.0, 2.0) < 1e-6,
            "identical WGS84 points must yield near-zero haversine distance"
        );
    }
}
