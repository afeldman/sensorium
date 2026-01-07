"""
Minimal experiment harness for sensorium.

Steps:
1) Push a handful of synthetic observations into Redis (no hard sync clocks).
2) Run SyncEngine.step() once to group them probabilistically.
3) Print resulting time slice(s).

Usage:
  python examples/minimal_experiment.py --redis-url redis://127.0.0.1/ --node-id node-a

Prereqs:
  - Redis running locally
  - sensorium Python bindings installed (e.g., `maturin develop`)
  - redis-py installed (`pip install redis`)
"""
import argparse
import json
import random
import time
from typing import List

import redis

from sensorium import SyncEngine


def write_observation(r: redis.Redis, sensor_id: str, t_local: float, sigma: float, payload_ref: str, ttl_seconds: int) -> None:
    key = f"obs:{sensor_id}:{int(t_local * 1e9)}"
    value = json.dumps(
        {
            "sensor_id": sensor_id,
            "sensor_type": "test",
            "t_local": t_local,
            "sigma": sigma,
            "payload_ref": payload_ref,
        }
    )
    r.set(key, value)
    r.expire(key, ttl_seconds)


def seed_observations(r: redis.Redis, sensor_ids: List[str], ttl_seconds: int, jitter_ms: float, seed: int) -> float:
    """Seed one observation per sensor around a common reference time (deterministic via seed)."""
    rng = random.Random(seed)
    t0 = time.time()
    for sid in sensor_ids:
        jitter = rng.uniform(-jitter_ms, jitter_ms) / 1000.0
        t_local = t0 + jitter
        write_observation(r, sid, t_local, sigma=0.05, payload_ref=f"mem://{sid}", ttl_seconds=ttl_seconds)
    return t0


def main() -> None:
    parser = argparse.ArgumentParser(description="Minimal sensorium experiment harness")
    parser.add_argument("--redis-url", default="redis://127.0.0.1/", help="Redis connection URL")
    parser.add_argument("--node-id", default="node-a", help="Node ID for leader election")
    parser.add_argument("--sensors", type=int, default=3, help="Number of synthetic sensors to seed")
    parser.add_argument("--ttl", type=int, default=30, help="TTL seconds for seeded observations")
    parser.add_argument("--jitter-ms", type=float, default=20.0, help="Uniform jitter (+/- ms) around now")
    parser.add_argument("--seed", type=int, default=1234, help="Random seed for reproducibility")
    args = parser.parse_args()

    r = redis.Redis.from_url(args.redis_url, decode_responses=True)

    sensor_ids = [f"s{i+1}" for i in range(max(1, args.sensors))]
    t0 = seed_observations(r, sensor_ids, ttl_seconds=args.ttl, jitter_ms=args.jitter_ms, seed=args.seed)
    print(f"Seeded {len(sensor_ids)} observations around t0={t0:.3f}s")

    engine = SyncEngine(args.redis_url, args.node_id, heartbeat_ttl=5)
    groups = engine.step()

    if not groups:
        print("No groups returned (possibly not master or empty redis)")
        return

    for g in groups:
        print(f"t_global={g['t_global']:.6f} :: members=")
        for m in g["members"]:
            print(f"  - sensor={m['sensor_id']:>4}  p={m['probability']:.6f}")


if __name__ == "__main__":
    main()
