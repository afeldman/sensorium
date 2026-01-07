//! # Sensor Core
//!
//! This crate provides the core data structures and functions for sensor data processing.
//! It defines the `SensorObservation` trait, which is a generic interface for sensor readings.
//! It also provides a `likelihood` function for statistical calculations.

pub trait SensorObservation {
    fn sensor_id(&self) -> &str;
    fn sensor_type(&self) -> &str;
    fn local_timestamp(&self) -> f64;
    fn payload(&self) -> &[u8];
    fn covariance(&self) -> f64;
}

/// A simple implementation of `SensorObservation` for demonstration purposes.
pub struct SimpleObservation<'a> {
    pub sensor_id: &'a str,
    pub sensor_type: &'a str,
    pub local_timestamp: f64,
    pub payload: &'a [u8],
    pub covariance: f64,
}

impl<'a> SensorObservation for SimpleObservation<'a> {
    fn sensor_id(&self) -> &str {
        self.sensor_id
    }

    fn sensor_type(&self) -> &str {
        self.sensor_type
    }

    fn local_timestamp(&self) -> f64 {
        self.local_timestamp
    }

    fn payload(&self) -> &[u8] {
        self.payload
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
/// assert_eq!(l, 3.558772643534344);
/// ```
pub fn likelihood(dt: f64, variance: f64) -> f64 {
    if variance <= 0.0 {
        return 0.0;
    }
    let sigma = variance.sqrt();
    let exponent = -0.5 * (dt / sigma).powi(2);
    1.0 / (sigma * (2.0 * std::f64::consts::PI).sqrt()) * exponent.exp()
}
