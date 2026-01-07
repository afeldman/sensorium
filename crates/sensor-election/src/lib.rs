//! # Sensor Election
//!
//! Bully-ähnliche Leader Election über Redis Heartbeats.
//! - Redis Keys: `election:bully:hb:*` (Heartbeats pro Node, mit TTL)
//! - Master ist der Node mit höchster `node_id` unter allen aktiven Heartbeats
//! - Schutzfunktion: Nur Master darf `sync:group:*` schreiben

use anyhow::{bail, Result};
use redis::{Commands, Connection};
use sensor_redis::{write_sync_group, SynchronizedGroup};

/// Baue den Heartbeat-Key für einen Node.
pub fn heartbeat_key(node_id: &str) -> String {
    format!("election:bully:hb:{}", node_id)
}

/// Sende Heartbeat und setze TTL zur Liveness-Erkennung.
pub fn send_heartbeat(con: &mut Connection, node_id: &str, ttl_seconds: usize) -> Result<()> {
    let key = heartbeat_key(node_id);
    con.set::<_, _, ()>(&key, 1)?;
    con.expire::<_, ()>(&key, ttl_seconds as i64)?;
    Ok(())
}

/// Bestimme aktuelle Master-Node-ID durch Auswahl des höchsten aktiven Heartbeat-Schlüssels.
pub fn current_master(con: &mut Connection) -> Result<Option<String>> {
    let keys: Vec<String> = con.keys("election:bully:hb:*")?;
    if keys.is_empty() {
        return Ok(None);
    }
    // Extrahiere node_id aus Key-Suffix
    let mut node_ids: Vec<String> = keys
        .into_iter()
        .filter_map(|k| k.rsplit(':').next().map(|s| s.to_string()))
        .collect();
    if node_ids.is_empty() {
        return Ok(None);
    }
    node_ids.sort();
    Ok(node_ids.pop())
}

/// Prüfe, ob dieser Node aktuell Master ist.
pub fn is_master(con: &mut Connection, node_id: &str) -> Result<bool> {
    let master = current_master(con)?;
    Ok(matches!(master.as_deref(), Some(id) if id == node_id))
}

/// Schutzfunktion: schreibe eine synchronisierte Gruppe nur, wenn `node_id` Master ist.
pub fn write_sync_group_if_master(
    con: &mut Connection,
    node_id: &str,
    group_id: &str,
    group: &SynchronizedGroup,
) -> Result<()> {
    if !is_master(con, node_id)? {
        bail!("not master: node '{}' cannot write sync:group:*", node_id);
    }
    write_sync_group(con, group_id, group)
}

#[cfg(test)]
mod tests {
    use super::*;
    use redis::Client;

    #[test]
    fn heartbeat_key_format() {
        assert_eq!(heartbeat_key("node-1"), "election:bully:hb:node-1");
    }

    fn get_con() -> Connection {
        let client = Client::open("redis://127.0.0.1/").unwrap();
        client.get_connection().unwrap()
    }

    fn flush() {
        let mut con = get_con();
        redis::cmd("FLUSHDB").execute(&mut con);
    }

    #[test]
    #[ignore]
    fn bully_selects_highest_active_node() {
        flush();
        let mut con = get_con();
        send_heartbeat(&mut con, "node-1", 10).unwrap();
        send_heartbeat(&mut con, "node-3", 10).unwrap();
        send_heartbeat(&mut con, "node-2", 10).unwrap();
        let master = current_master(&mut con).unwrap();
        assert_eq!(master.as_deref(), Some("node-3"));
        assert!(is_master(&mut con, "node-3").unwrap());
        assert!(!is_master(&mut con, "node-2").unwrap());
    }
}
