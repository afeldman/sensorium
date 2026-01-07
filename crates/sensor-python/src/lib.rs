//! # Sensor Python
//!
//! Python-Bindings für die probabilistische Synchronisation.
//! Exponiert `SyncEngine` mit `step()` → gibt synchronisierte Gruppen
//! als Python-freundliche Strukturen zurück.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use redis::Client;
use sensor_election::{is_master, send_heartbeat, write_sync_group_if_master};
use sensor_redis::{get_all_raw_observations, read_time_sync_state, SynchronizedGroup};
use sensor_sync::{group_observations_probabilistically, TimeOffsetModel};
use std::collections::HashMap;

#[pyclass]
struct SyncEngine {
    redis_url: String,
    node_id: String,
    heartbeat_ttl: usize,
}

fn to_py_group(py: Python<'_>, group: &SynchronizedGroup) -> PyResult<Py<PyAny>> {
    let out = pyo3::types::PyDict::new(py);
    out.set_item("t_global", group.t_global)?;
    let members = pyo3::types::PyList::empty(py);
    for m in &group.members {
        let md = pyo3::types::PyDict::new(py);
        md.set_item("sensor_id", &m.sensor_id)?;
        md.set_item("probability", m.probability)?;
        members.append(md)?;
    }
    out.set_item("members", members)?;
    Ok(out.unbind().into_any())
}

#[pymethods]
impl SyncEngine {
    #[new]
    fn new(redis_url: &str, node_id: &str, heartbeat_ttl: Option<usize>) -> Self {
        SyncEngine {
            redis_url: redis_url.to_string(),
            node_id: node_id.to_string(),
            heartbeat_ttl: heartbeat_ttl.unwrap_or(5),
        }
    }

    /// Führe einen Synchronisationsschritt aus und liefere eine Liste von Gruppen.
    /// Jede Gruppe ist ein Dict mit `t_global: float` und `members: List[Dict]`.
    fn step(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        // Redis verbinden
        let client = Client::open(self.redis_url.as_str())
            .map_err(|e| PyRuntimeError::new_err(format!("redis client error: {e}")))?;
        let mut con = client
            .get_connection()
            .map_err(|e| PyRuntimeError::new_err(format!("redis connection error: {e}")))?;

        // Heartbeat senden
        send_heartbeat(&mut con, &self.node_id, self.heartbeat_ttl)
            .map_err(|e| PyRuntimeError::new_err(format!("heartbeat error: {e}")))?;

        // Rohbeobachtungen laden
        let observations = get_all_raw_observations(&mut con)
            .map_err(|e| PyRuntimeError::new_err(format!("read observations error: {e}")))?;

        if observations.is_empty() { return Ok(vec![]); }

        // TimeSyncState je Sensor cachen
        let mut cache: HashMap<String, TimeOffsetModel> = HashMap::new();
        let mut models = Vec::with_capacity(observations.len());
        for obs in &observations {
            let entry = cache.entry(obs.sensor_id.clone()).or_insert_with(|| {
                match read_time_sync_state(&mut con, &obs.sensor_id) {
                    Ok(state) => TimeOffsetModel::from(state),
                    Err(_) => TimeOffsetModel { offset_mean: 0.0, offset_var: 0.1, drift: 1.0 },
                }
            });
            models.push(entry.clone());
        }

        // Eine Gruppe für diesen Batch bilden
        let group = group_observations_probabilistically(&observations, &models)
            .map_err(|e| PyRuntimeError::new_err(format!("grouping error: {e}")))?;

        // Schreiben nur wenn Master
        if is_master(&mut con, &self.node_id)
            .map_err(|e| PyRuntimeError::new_err(format!("is_master error: {e}")))? {
            // group_id deterministisch aus Zeit ableiten
            let group_id = format!("g:{}", (group.t_global * 1e9).round() as i128);
            write_sync_group_if_master(&mut con, &self.node_id, &group_id, &group)
                .map_err(|e| PyRuntimeError::new_err(format!("write group error: {e}")))?;
        }

        // In Python-Objekt wandeln (Liste von 1 Gruppe aktuell)
        let py_group = to_py_group(py, &group)?;
        Ok(vec![py_group])
    }
}

#[pymodule]
fn sensorium(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SyncEngine>()?;
    Ok(())
}
