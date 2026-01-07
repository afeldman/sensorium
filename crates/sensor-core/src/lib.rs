//! # Sensor Core
//!
//! This crate provides the core data structures and functions for sensor data processing.
//! It defines the `SensorObservation` trait, which is a generic interface for sensor readings,
//! and a concrete `Observation` struct.
//! It also provides a `likelihood` function for statistical calculations.

pub trait SensorObservation {
    fn sensor_id(&self) -> &str;
    fn sensor_type(&self) -> &str;
    fn local_timestamp(&self) -> f64;
    fn payload(&self) -> &[u8];
    fn covariance(&self) -> f64;
}

/// A concrete implementation of `SensorObservation`.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub sensor_id: String,
    pub sensor_type: String,
    pub local_timestamp: f64,
    pub payload: Vec<u8>,
    pub covariance: f64,
}

impl SensorObservation for Observation {
    fn sensor_id(&self) -> &str {
        &self.sensor_id
    }

    fn sensor_type(&self) -> &str {
        &self.sensor_type
    }

    fn local_timestamp(&self) -> f64 {
        self.local_timestamp
    }

    fn payload(&self) -> &[u8] {
        &self.payload
    }

    fn covariance(&self) -> f64 {
        self.covariance
    }
}

/// Calculates the likelihood of an observation given a time difference and variance.
///
/// # Examples
///
/// ```
/// use sensor_core::likelihood;
///
/// let l = likelihood(0.1, 0.01);
/// // Gaussian PDF at x=0.1 with variance=0.01 (sigma=0.1)
/// assert!((l - 2.419707245).abs() < 1e-6);
/// ```
pub fn likelihood(dt: f64, variance: f64) -> f64 {
    if variance <= 0.0 {
        return 0.0;
    }
    let sigma = variance.sqrt();
    let exponent = -0.5 * (dt / sigma).powi(2);
    1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt()) * exponent.exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_trait_impl_works() {
        let obs = Observation {
            sensor_id: "sensor1".to_string(),
            sensor_type: "temp".to_string(),
            local_timestamp: 12345.6789,
            payload: vec![1, 2, 3],
            covariance: 0.25,
        };

        assert_eq!(obs.sensor_id(), "sensor1");
        assert_eq!(obs.sensor_type(), "temp");
        assert_eq!(obs.local_timestamp(), 12345.6789);
        assert_eq!(obs.payload(), &[1, 2, 3]);
        assert_eq!(obs.covariance(), 0.25);
    }

    #[test]
    fn likelihood_matches_gaussian_pdf() {
        let l = likelihood(0.1, 0.01);
        assert!((l - 2.419707245).abs() < 1e-9);
    }

    #[test]
    fn likelihood_handles_nonpositive_variance() {
        assert_eq!(likelihood(0.0, 0.0), 0.0);
        assert_eq!(likelihood(1.0, -1.0), 0.0);
    }
}
