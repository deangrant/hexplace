//! Ranking helpers for forward and reverse results.

use geofind_core::Place;

/// Combines text relevance with place importance.
pub fn forward_score(text_score: f32, place: &Place) -> f32 {
    text_score + place.importance
}

/// Haversine distance in meters between two WGS84 points.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let to_rad = |d: f64| d.to_radians();
    let dlat = to_rad(lat2 - lat1);
    let dlon = to_rad(lon2 - lon1);
    let a = (dlat / 2.0).sin().powi(2)
        + to_rad(lat1).cos() * to_rad(lat2).cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

/// Scores a reverse candidate: closer and more important ranks higher.
pub fn reverse_score(distance_m: f64, importance: f32) -> f32 {
    let distance_term = 1.0 / (1.0 + (distance_m as f32 / 50.0));
    distance_term + importance * 0.25
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closer_scores_higher() {
        let near = reverse_score(10.0, 0.1);
        let far = reverse_score(500.0, 0.1);
        assert!(near > far);
    }

    #[test]
    fn haversine_zero_for_same_point() {
        assert!(haversine_m(1.0, 2.0, 1.0, 2.0) < 1e-6);
    }
}
