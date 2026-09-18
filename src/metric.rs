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
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f32]) -> f32 {
    dot(v, v).sqrt()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let denom = norm(a) * norm(b);
    if denom == 0.0 {
        return 0.0;
    }
    1.0 - (dot(a, b) / denom)
}

fn euclidean(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
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
}
