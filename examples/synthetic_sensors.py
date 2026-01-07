#!/usr/bin/env python3
"""
End-to-End Beispiel: Synthetische Sensor-Streams mit Drift & Jitter
erzeugen, in Redis schreiben und mit SyncEngine synchronisieren.
"""
import json
import time
import redis
from dataclasses import dataclass
from typing import List
import random


@dataclass
class SyntheticSensor:
    sensor_id: str
    sensor_type: str
    offset: float  # Zeitoffset in Sekunden
    drift: float   # Drift-Faktor (1.0 = keine Drift)
    jitter: float  # Standardabweichung des Messrauschens


def simulate_observation(sensor: SyntheticSensor, true_global_time: float) -> dict:
    """Simuliere eine Beobachtung mit Offset, Drift und Jitter."""
    # Inverse mapping: t_local = (t_global - offset) / drift
    t_local = (true_global_time - sensor.offset) / sensor.drift
    # Füge Jitter hinzu
    t_local += random.gauss(0, sensor.jitter)
    
    return {
        "sensor_id": sensor.sensor_id,
        "sensor_type": sensor.sensor_type,
        "t_local": t_local,
        "sigma": sensor.jitter,
        "payload_ref": f"mem://{sensor.sensor_id}/{int(t_local*1e9)}",
    }


def main():
    # Redis verbinden
    r = redis.Redis(host='localhost', port=6379, db=0, decode_responses=True)
    r.flushdb()
    print("🗑️  Redis DB geflusht")
    
    # Definiere synthetische Sensoren
    sensors = [
        SyntheticSensor("camera-1", "camera", offset=0.05, drift=1.0001, jitter=0.01),
        SyntheticSensor("imu-1", "imu", offset=-0.02, drift=0.9999, jitter=0.02),
        SyntheticSensor("mic-1", "microphone", offset=0.01, drift=1.0, jitter=0.005),
    ]
    
    # Simuliere ein Ereignis zur Zeit t=10.0
    true_event_time = 10.0
    print(f"\n📡 Simuliere Ereignis zur globalen Zeit t={true_event_time}")
    
    observations = []
    for sensor in sensors:
        obs = simulate_observation(sensor, true_event_time)
        observations.append(obs)
        
        # Schreibe in Redis
        key = f"obs:{obs['sensor_id']}:{int(obs['t_local']*1e9)}"
        r.setex(key, 60, json.dumps(obs))
        print(f"  ✓ {sensor.sensor_id}: t_local={obs['t_local']:.6f}s, σ={obs['sigma']}")
    
    # Schreibe TimeSyncState für jeden Sensor (initialwerte)
    for sensor in sensors:
        state = {
            "offset_mean": 0.0,
            "offset_var": 0.1,
            "drift": 1.0,
        }
        key = f"sync:state:{sensor.sensor_id}"
        r.set(key, json.dumps(state))
    
    print("\n⏳ Warte kurz, dann importiere sensorium...")
    time.sleep(0.5)
    
    # Importiere Python-Bindings
    try:
        from sensorium import SyncEngine
    except ImportError:
        print("❌ sensorium nicht gefunden. Bitte zuerst ausführen:")
        print("   maturin develop")
        return
    
    # Erstelle SyncEngine
    engine = SyncEngine("redis://127.0.0.1/", "master-node", 10)
    print("🚀 SyncEngine initialisiert")
    
    # Führe Synchronisationsschritt aus
    groups = engine.step()
    
    print(f"\n✨ Synchronisierung abgeschlossen: {len(groups)} Gruppe(n)")
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
