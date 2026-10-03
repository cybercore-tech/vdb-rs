//! Distance/similarity metrics used for vector search and scoring.

/// A similarity/distance metric usable for ANN search and scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Cosine distance (`1 - cosine similarity`).
    Cosine,
    /// Euclidean (L2) distance.
    Euclidean,
    /// Dot product.
    DotProduct,
}

impl Metric {
    /// Compute this metric between two equal-length `f32` vectors.
    ///
    /// # Panics
    ///
    /// Panics if `a.len() != b.len()` in debug builds — dimension
    /// mismatches are expected to be caught earlier, at the collection
    /// boundary (see [`crate::Error::DimensionMismatch`]).
    pub fn distance(self, a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len());
        match self {
            Metric::Cosine => cosine(a, b),
            Metric::Euclidean => euclidean(a, b),
            Metric::DotProduct => dot(a, b),
        }
    }

    /// Compare scores from best to worst for this metric.
    #[cfg(all(feature = "storage", feature = "serde-query"))]
    pub(crate) fn compare(self, a: f32, b: f32) -> std::cmp::Ordering {
        if self.higher_is_better() {
            b.total_cmp(&a)
        } else {
            a.total_cmp(&b)
        }
    }

    /// The lowercase name this metric is stored as in a `CollectionConfig`
    /// (see `crate::kv::CollectionConfig::metric`, the inverse of this).
    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Cosine => "cosine",
            Metric::Euclidean => "euclidean",
            Metric::DotProduct => "dot_product",
        }
    }

    /// Does a *higher* [`Metric::distance`] value mean "more similar" for
    /// this metric?
    ///
    /// `Cosine`/`Euclidean` are genuine distances (lower = closer);
    /// `DotProduct` is a raw similarity score (higher = closer) — ranking
    /// query results with a single ascending sort would silently invert
    /// `DotProduct` results. Callers doing top-k ranking must check this.
    pub fn higher_is_better(self) -> bool {
        matches!(self, Metric::DotProduct)
    }
}

fn dot64(a: &[f32], b: &[f32]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| f64::from(x) * f64::from(y))
        .sum()
}
fn dot(a: &[f32], b: &[f32]) -> f32 {
    dot64(a, b) as f32
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let denom = dot64(a, a).sqrt() * dot64(b, b).sqrt();
    if denom == 0.0 {
        return 1.0;
    }
    (1.0 - (dot64(a, b) / denom).clamp(-1.0, 1.0)) as f32
}
fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| (f64::from(x) - f64::from(y)).powi(2))
        .sum::<f64>()
        .sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_product_of_orthogonal_vectors_is_zero() {
        assert_eq!(Metric::DotProduct.distance(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
    }

    #[test]
    fn cosine_distance_of_identical_vectors_is_zero() {
        let v = [1.0, 2.0, 3.0];
        assert!(Metric::Cosine.distance(&v, &v).abs() < 1e-6);
    }

    #[test]
    fn euclidean_distance_matches_known_3_4_5_triangle() {
        assert_eq!(Metric::Euclidean.distance(&[0.0, 0.0], &[3.0, 4.0]), 5.0);
    }

    #[test]
    fn finite_extreme_vectors_do_not_produce_nan_scores() {
        let vector = [f32::MAX, f32::MAX];
        assert_eq!(Metric::Cosine.distance(&vector, &vector), 0.0);
        assert_eq!(Metric::Euclidean.distance(&vector, &vector), 0.0);
        assert!(!Metric::DotProduct.distance(&vector, &vector).is_nan());
    }

    #[test]
    fn zero_norm_cosine_vectors_do_not_rank_as_perfect_matches() {
        assert_eq!(Metric::Cosine.distance(&[0.0, 0.0], &[1.0, 0.0]), 1.0);
        assert_eq!(Metric::Cosine.distance(&[0.0, 0.0], &[0.0, 0.0]), 1.0);
    }

    #[test]
    fn only_dot_product_ranks_higher_as_better() {
        assert!(!Metric::Cosine.higher_is_better());
        assert!(!Metric::Euclidean.higher_is_better());
        assert!(Metric::DotProduct.higher_is_better());
    }

    #[cfg(feature = "storage")]
    #[test]
    fn as_str_round_trips_through_collection_config_metric() {
        for m in [Metric::Cosine, Metric::Euclidean, Metric::DotProduct] {
            let config = crate::kv::CollectionConfig {
                dim: 1,
                metric: m.as_str().to_string(),
                next_id: 0,
                generation: 0,
                revision: 0,
            };
            assert_eq!(config.metric(), Some(m));
        }
    }
}
