//! # Sensor Redis
//!
//! This crate defines the data structures that are stored in Redis
//! and provides functions for interacting with the Redis database.
use anyhow::Result;
use redis::{Commands, Connection};
use serde::{Deserialize, Serialize};

// --- Data Structures ---

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct RawObservation {
    pub sensor_id: String,
    pub sensor_type: String,
    pub t_local: f64,
    pub sigma: f64,
    pub payload_ref: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct TimeSyncState {
    pub offset_mean: f64,
    pub offset_var: f64,
    pub drift: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct GroupMember {
    pub sensor_id: String,
    pub probability: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SynchronizedGroup {
    pub t_global: f64,
    pub members: Vec<GroupMember>,
}

// --- Key Builders ---

pub fn raw_observation_key(sensor_id: &str, timestamp: f64) -> String {
    let timestamp_ns = (timestamp * 1e9) as u64;
    format!("obs:{}:{}", sensor_id, timestamp_ns)
}

pub fn time_sync_state_key(sensor_id: &str) -> String {
    format!("sync:state:{}", sensor_id)
}

pub fn sync_group_key(group_id: &str) -> String {
    format!("sync:group:{}", group_id)
}

// --- Generic I/O Helpers ---

fn write_struct<T: Serialize>(
    con: &mut Connection,
    key: &str,
    value: &T,
) -> Result<()> {
    let json_string = serde_json::to_string(value)?;
    con.set::<_, _, ()>(key, json_string)?;
    Ok(())
}

fn read_struct<T: for<'de> Deserialize<'de>>(
    con: &mut Connection,
    key: &str,
) -> Result<T> {
    let json_string: String = con.get(key)?;
    let value: T = serde_json::from_str(&json_string)?;
    Ok(value)
}

// --- Read/Write Functions ---

pub fn write_raw_observation(
    con: &mut Connection,
    observation: &RawObservation,
    ttl_seconds: usize,
) -> Result<()> {
    let key = raw_observation_key(&observation.sensor_id, observation.t_local);
    write_struct(con, &key, observation)?;
    con.expire::<_, ()>(&key, ttl_seconds as i64)?;
    Ok(())
}

pub fn read_time_sync_state(con: &mut Connection, sensor_id: &str) -> Result<TimeSyncState> {
    let key = time_sync_state_key(sensor_id);
    read_struct(con, &key)
}

pub fn write_time_sync_state(
    con: &mut Connection,
    sensor_id: &str,
    state: &TimeSyncState,
) -> Result<()> {
    let key = time_sync_state_key(sensor_id);
    write_struct(con, &key, state)
}

pub fn read_sync_group(con: &mut Connection, group_id: &str) -> Result<SynchronizedGroup> {
    let key = sync_group_key(group_id);
    read_struct(con, &key)
}

pub fn write_sync_group(
    con: &mut Connection,
    group_id: &str,
    group: &SynchronizedGroup,
) -> Result<()> {
    let key = sync_group_key(group_id);
    write_struct(con, &key, group)
}

pub fn get_all_raw_observations(con: &mut Connection) -> Result<Vec<RawObservation>> {
    let mut observations = Vec::new();
    let keys: Vec<String> = con.keys("obs:*")?;
    if keys.is_empty() {
        return Ok(observations);
    }
    let values: Vec<String> = con.get(keys)?;
    for val in values {
        let obs: RawObservation = serde_json::from_str(&val)?;
        observations.push(obs);
    }
    Ok(observations)
}


#[cfg(test)]
mod tests {
    use super::*;
    use redis::Client;

    // NOTE: These tests require a running Redis server on the default port (6379).
    // Run them with `cargo test -- --ignored`.

    fn get_redis_connection() -> Connection {
        let client = Client::open("redis://127.0.0.1/").unwrap();
        client.get_connection().unwrap()
    }

    fn flush_db() {
        let mut con = get_redis_connection();
        redis::cmd("FLUSHDB").execute(&mut con);
    }

    #[test]
    fn test_key_builders() {
        assert_eq!(raw_observation_key("sensor1", 123.456), "obs:sensor1:123456000000");
        assert_eq!(time_sync_state_key("sensor1"), "sync:state:sensor1");
        assert_eq!(sync_group_key("group-abc"), "sync:group:group-abc");
    }

    #[test]
    #[ignore]
    fn test_raw_observation_io() {
        flush_db();
        let mut con = get_redis_connection();

        let obs = RawObservation {
            sensor_id: "sensor-alpha".to_string(),
            sensor_type: "camera".to_string(),
            t_local: 9876.5432,
            sigma: 0.05,
            payload_ref: "s3://bucket/img1.jpg".to_string(),
        };

        assert!(write_raw_observation(&mut con, &obs, 10).is_ok());

        let all_obs = get_all_raw_observations(&mut con).unwrap();
        assert_eq!(all_obs.len(), 1);
        assert_eq!(all_obs[0], obs);

        let key = raw_observation_key(&obs.sensor_id, obs.t_local);
        let ttl: isize = con.ttl(&key).unwrap();
        assert!(ttl > 0 && ttl <= 10);
    }

    #[test]
    #[ignore]
    fn test_time_sync_state_io() {
        flush_db();
        let mut con = get_redis_connection();

        let state = TimeSyncState {
            offset_mean: -0.1,
            offset_var: 0.002,
            drift: 1.00001,
        };
        let sensor_id = "sensor-beta";

        assert!(write_time_sync_state(&mut con, sensor_id, &state).is_ok());

        let read_state = read_time_sync_state(&mut con, sensor_id).unwrap();
        assert_eq!(read_state, state);
    }

    #[test]
    #[ignore]
    fn test_sync_group_io() {
        flush_db();
        let mut con = get_redis_connection();

        let group = SynchronizedGroup {
            t_global: 12345.678,
            members: vec![
                GroupMember { sensor_id: "sensor-gamma".to_string(), probability: 0.95 },
                GroupMember { sensor_id: "sensor-delta".to_string(), probability: 0.88 },
            ]
        };
        let group_id = "group-xyz";

        assert!(write_sync_group(&mut con, group_id, &group).is_ok());

        let read_group = read_sync_group(&mut con, group_id).unwrap();
        assert_eq!(read_group, group);
    }
}
