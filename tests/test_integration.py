"""
End-to-End Integration Tests für das Sensorium-Projekt.
Erfordert laufenden Redis-Server auf localhost:6379.
"""
import pytest
import redis
import json
from sensorium import SyncEngine


@pytest.fixture
def redis_client():
    """Redis-Client für Tests."""
    r = redis.Redis(host='localhost', port=6379, db=15, decode_responses=True)
    r.flushdb()
    yield r
    r.flushdb()


def test_empty_redis_returns_empty_groups(redis_client):
    """Test: Leere Redis-DB sollte keine Gruppen liefern."""
    engine = SyncEngine("redis://127.0.0.1:6379/15", "test-node", 5)
    groups = engine.step()
    assert groups == []


def test_single_sensor_observation(redis_client):
    """Test: Einzelne Beobachtung sollte Gruppe mit einem Mitglied bilden."""
    # Schreibe eine Beobachtung
    obs = {
        "sensor_id": "sensor-1",
        "sensor_type": "camera",
        "t_local": 10.0,
        "sigma": 0.01,
        "payload_ref": "mem://test",
    }
    key = f"obs:{obs['sensor_id']}:{int(obs['t_local']*1e9)}"
    redis_client.setex(key, 60, json.dumps(obs))
    
    # Synchronisiere
    engine = SyncEngine("redis://127.0.0.1:6379/15", "test-node", 5)
    groups = engine.step()
    
    assert len(groups) == 1
    group = groups[0]
    assert abs(group['t_global'] - 10.0) < 0.1
    assert len(group['members']) == 1
    assert group['members'][0]['sensor_id'] == 'sensor-1'
    assert group['members'][0]['probability'] == 1.0


def test_multiple_sensors_same_event(redis_client):
    """Test: Mehrere Sensoren am gleichen Ereignis."""
    observations = [
        {"sensor_id": "cam-1", "sensor_type": "camera", "t_local": 10.0, "sigma": 0.01},
        {"sensor_id": "imu-1", "sensor_type": "imu", "t_local": 10.02, "sigma": 0.02},
        {"sensor_id": "mic-1", "sensor_type": "mic", "t_local": 9.98, "sigma": 0.015},
    ]
    
    for obs in observations:
        obs["payload_ref"] = f"mem://{obs['sensor_id']}"
        key = f"obs:{obs['sensor_id']}:{int(obs['t_local']*1e9)}"
        redis_client.setex(key, 60, json.dumps(obs))
    
    # Synchronisiere
    engine = SyncEngine("redis://127.0.0.1:6379/15", "test-node", 5)
    groups = engine.step()
    
    assert len(groups) == 1
    group = groups[0]
    
    # Geschätzte Zeit sollte nahe 10.0 sein
    assert abs(group['t_global'] - 10.0) < 0.1
    
    # Alle 3 Sensoren sollten Mitglieder sein
    assert len(group['members']) == 3
    
    # Summe der Wahrscheinlichkeiten sollte 1.0 sein
    total_prob = sum(m['probability'] for m in group['members'])
    assert abs(total_prob - 1.0) < 1e-6


def test_probabilistic_membership(redis_client):
    """Test: Probabilistische Mitgliedschaft ohne harte Schwellwerte."""
    # Zwei Beobachtungen: eine sehr nah, eine weiter weg
    observations = [
        {"sensor_id": "sensor-close", "sensor_type": "x", "t_local": 10.0, "sigma": 0.01},
        {"sensor_id": "sensor-far", "sensor_type": "x", "t_local": 10.5, "sigma": 0.01},
    ]
    
    for obs in observations:
        obs["payload_ref"] = "mem://test"
        key = f"obs:{obs['sensor_id']}:{int(obs['t_local']*1e9)}"
        redis_client.setex(key, 60, json.dumps(obs))
    
    engine = SyncEngine("redis://127.0.0.1:6379/15", "test-node", 5)
    groups = engine.step()
    
    assert len(groups) == 1
    group = groups[0]
    assert len(group['members']) == 2
    
    # Beide sollten Wahrscheinlichkeit > 0 haben (keine Schwellwerte!)
    for member in group['members']:
        assert member['probability'] > 0.0
    
    # Die nähere Beobachtung sollte höhere Wahrscheinlichkeit haben
    probs = {m['sensor_id']: m['probability'] for m in group['members']}
    # Dies hängt vom geschätzten t_global ab, aber generell sollte sensor-close höher sein
    # wenn t_global näher an 10.0 liegt
    if group['t_global'] < 10.25:
        assert probs['sensor-close'] > probs['sensor-far']


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
