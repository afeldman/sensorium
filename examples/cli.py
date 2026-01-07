#!/usr/bin/env python3
"""
Einfaches CLI-Tool zum Einlesen von Sensor-Daten und Synchronisieren.
"""
import argparse
import json
import redis
from sensorium import SyncEngine


def ingest_observation(redis_client, sensor_id: str, sensor_type: str, 
                      t_local: float, sigma: float, payload_ref: str, ttl: int = 60):
    """Schreibe eine Beobachtung in Redis."""
    obs = {
        "sensor_id": sensor_id,
        "sensor_type": sensor_type,
        "t_local": t_local,
        "sigma": sigma,
        "payload_ref": payload_ref,
    }
    key = f"obs:{sensor_id}:{int(t_local*1e9)}"
    redis_client.setex(key, ttl, json.dumps(obs))
    print(f"✓ Ingested: {sensor_id} @ t={t_local:.6f}s")


def sync_observations(redis_url: str, node_id: str):
    """Synchronisiere alle Beobachtungen in Redis."""
    engine = SyncEngine(redis_url, node_id, 10)
    groups = engine.step()
    
    print(f"\n📊 Synchronization Result: {len(groups)} group(s)")
    for i, group in enumerate(groups):
        print(f"\nGroup {i+1}:")
        print(f"  Global Time: {group['t_global']:.6f}s")
        print(f"  Members: {len(group['members'])}")
        for member in group['members']:
            print(f"    - {member['sensor_id']}: P={member['probability']:.4f}")


def main():
    parser = argparse.ArgumentParser(description="Sensorium CLI - Sensor Data Ingestion & Sync")
    parser.add_argument("--redis", default="redis://127.0.0.1/", help="Redis URL")
    parser.add_argument("--node-id", default="cli-node", help="Node ID for election")
    
    subparsers = parser.add_subparsers(dest="command", required=True)
    
    # Ingest command
    ingest_parser = subparsers.add_parser("ingest", help="Ingest a sensor observation")
    ingest_parser.add_argument("sensor_id", help="Sensor ID")
    ingest_parser.add_argument("sensor_type", help="Sensor type")
    ingest_parser.add_argument("t_local", type=float, help="Local timestamp")
    ingest_parser.add_argument("--sigma", type=float, default=0.01, help="Measurement uncertainty")
    ingest_parser.add_argument("--payload", default="", help="Payload reference")
    
    # Sync command
    sync_parser = subparsers.add_parser("sync", help="Synchronize observations")
    
    args = parser.parse_args()
    
    if args.command == "ingest":
        r = redis.Redis.from_url(args.redis, decode_responses=True)
        ingest_observation(r, args.sensor_id, args.sensor_type, 
                         args.t_local, args.sigma, args.payload)
    
    elif args.command == "sync":
        sync_observations(args.redis, args.node_id)


if __name__ == "__main__":
    main()
