//! # Sensor Redis
//!
//! This crate defines the data structures that are stored in Redis.
//! These structures are used for data exchange and storing the state of the sensor network.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct RawObservation {
    pub sensor_type: String,
    pub t_local: f64,
    pub sigma: f64,
    pub payload_ref: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TimeSyncState {
    pub offset_mean: f64,
    pub offset_var: f64,
    pub drift: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GroupMember {
    pub sensor_id: String,
    pub probability: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SynchronizedGroup {
    pub t_global: f64,
    pub members: Vec<GroupMember>,
}
