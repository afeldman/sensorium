#!/usr/bin/env python3
"""
End-to-End Beispiel: Synthetische Sensor-Streams mit Drift & Jitter
erzeugen, in Redis schreiben und mit SyncEngine synchronisieren.
"""
import json
import time
import redis
from dataclasses import dataclass
from typing import List, Optional
import random


@dataclass
class SyntheticSensor:
    sensor_id: str
    sensor_type: str
    offset: float  # Zeitoffset in Sekunden
    drift: float   # Drift-Faktor (1.0 = keine Drift)
    jitter: float  # Standardabweichung des Messrauschens


@dataclass
class SimulationConfig:
    ttl_seconds: int = 60
    jitter_ms: float = 10.0
    redis_url: str = "redis://127.0.0.1/"
    node_id: str = "master-node"
    heartbeat_ttl: int = 10


def simulate_observation(sensor: SyntheticSensor, true_global_time: float, rng: random.Random) -> dict:
    """Simuliere eine Beobachtung mit Offset, Drift und Jitter."""
    # Inverse mapping: t_local = (t_global - offset) / drift
    t_local = (true_global_time - sensor.offset) / sensor.drift
    # Füge Jitter hinzu
    t_local += rng.gauss(0, sensor.jitter)
    
    return {
        "sensor_id": sensor.sensor_id,
        "sensor_type": sensor.sensor_type,
        "t_local": t_local,
        "sigma": sensor.jitter,
        "payload_ref": f"mem://{sensor.sensor_id}/{int(t_local*1e9)}",
    }


def run_simulation(seed: int, config: SimulationConfig, sensors: Optional[List[SyntheticSensor]] = None):
    """Deterministic simulation entry point. Seedable and reusable.

    Returns: (groups, true_event_time)
    """
    rng = random.Random(seed)

    # Redis verbinden
    r = redis.Redis.from_url(config.redis_url, decode_responses=True)
    r.flushdb()
    
    # Definiere synthetische Sensoren
    sensors = sensors or [
        SyntheticSensor("camera-1", "camera", offset=0.05, drift=1.0001, jitter=0.01),
        SyntheticSensor("imu-1", "imu", offset=-0.02, drift=0.9999, jitter=0.02),
        SyntheticSensor("mic-1", "microphone", offset=0.01, drift=1.0, jitter=0.005),
    ]
    
    # Simuliere ein Ereignis zur Zeit t=10.0
    true_event_time = 10.0
    
    observations = []
    for sensor in sensors:
        obs = simulate_observation(sensor, true_event_time, rng)
        observations.append(obs)
        
        # Schreibe in Redis
        key = f"obs:{obs['sensor_id']}:{int(obs['t_local']*1e9)}"
        r.setex(key, config.ttl_seconds, json.dumps(obs))
    
    # Schreibe TimeSyncState für jeden Sensor (initialwerte)
    for sensor in sensors:
        state = {
            "offset_mean": 0.0,
            "offset_var": 0.1,
            "drift": 1.0,
        }
        key = f"sync:state:{sensor.sensor_id}"
        r.set(key, json.dumps(state))
    
    # Importiere Python-Bindings
    from sensorium import SyncEngine
    
    # Erstelle SyncEngine
    engine = SyncEngine(config.redis_url, config.node_id, config.heartbeat_ttl)
    
    # Führe Synchronisationsschritt aus
    groups = engine.step()
    return groups, true_event_time


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Deterministic synthetic sensor simulation")
    parser.add_argument("--seed", type=int, default=42, help="Random seed for reproducibility")
    parser.add_argument("--redis-url", default="redis://127.0.0.1/", help="Redis connection URL")
    parser.add_argument("--node-id", default="master-node", help="Node ID for election")
    parser.add_argument("--ttl", type=int, default=60, help="TTL seconds for observations")
    parser.add_argument("--jitter-ms", type=float, default=10.0, help="Uniform jitter (+/- ms) for seeding")

    args = parser.parse_args()

    cfg = SimulationConfig(
        ttl_seconds=args.ttl,
        jitter_ms=args.jitter_ms,
        redis_url=args.redis_url,
        node_id=args.node_id,
    )

    groups, true_event_time = run_simulation(args.seed, cfg)

    print(f"✨ Synchronisierung abgeschlossen: {len(groups)} Gruppe(n)")
    for i, group in enumerate(groups):
        print(f"\n  Gruppe {i+1}:")
        print(f"    t_global: {group['t_global']:.6f}s (Wahrer Wert: {true_event_time:.6f}s)")
        print(f"    Abweichung: {abs(group['t_global'] - true_event_time)*1000:.3f}ms")
        print(f"    Mitglieder:")
        for member in group['members']:
            print(f"      - {member['sensor_id']}: P={member['probability']:.4f}")
    
    # Validierung
    if groups:
        group = groups[0]
        error_ms = abs(group['t_global'] - true_event_time) * 1000
        if error_ms < 50:  # Toleranz: 50ms
            print(f"\n✅ Test bestanden! Schätzfehler: {error_ms:.3f}ms")
        else:
            print(f"\n⚠️  Schätzfehler zu groß: {error_ms:.3f}ms")


if __name__ == "__main__":
    main()
