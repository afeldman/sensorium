"""Common utilities for deterministic Sensorium experiments.

- Uses SyncEngine from the sensorium Python bindings
- Seeds synthetic observations into Redis deterministically
- Provides helpers for alignment error and probability analysis

All randomness is controlled by a `random.Random` instance passed in.
"""
import json
import math
import random
from dataclasses import dataclass
from typing import Iterable, List, Tuple

import redis
from sensorium import SyncEngine


@dataclass
class SensorSpec:
    sensor_id: str
    sensor_type: str
    offset: float  # seconds
    drift: float   # dimensionless (1.0 = no drift)
    jitter: float  # seconds (std dev)


@dataclass
class ExperimentConfig:
    redis_url: str = "redis://127.0.0.1/"
    heartbeat_ttl: int = 5
    ttl_seconds: int = 30
    bucket_size_ms: int = 1000


def flush_db(redis_url: str) -> redis.Redis:
    r = redis.Redis.from_url(redis_url, decode_responses=True)
    r.flushdb()
    return r


def _write_observation(r: redis.Redis, obs: dict, ttl_seconds: int) -> None:
    key = f"obs:{obs['sensor_id']}:{int(obs['t_local'] * 1e9)}"
    r.setex(key, ttl_seconds, json.dumps(obs))


def _write_state(r: redis.Redis, sensor_id: str, offset_mean: float = 0.0, offset_var: float = 0.1, drift: float = 1.0) -> None:
    key = f"sync:state:{sensor_id}"
    state = {"offset_mean": offset_mean, "offset_var": offset_var, "drift": drift}
    r.set(key, json.dumps(state))


def seed_observations(
    r: redis.Redis,
    sensors: Iterable[SensorSpec],
    true_time: float,
    rng: random.Random,
    ttl_seconds: int,
) -> None:
    """Seed one observation per sensor around `true_time`.

    Local time is computed as inverse mapping of drift/offset plus Gaussian jitter.
    """
    for s in sensors:
        t_local = (true_time - s.offset) / s.drift
        t_local += rng.gauss(0.0, s.jitter)
        obs = {
            "sensor_id": s.sensor_id,
            "sensor_type": s.sensor_type,
            "t_local": t_local,
            "sigma": s.jitter,
            "payload_ref": f"mem://{s.sensor_id}/{int(t_local*1e9)}",
        }
        _write_observation(r, obs, ttl_seconds=ttl_seconds)
        _write_state(r, s.sensor_id, offset_mean=0.0, offset_var=0.1, drift=1.0)


def run_sync(redis_url: str, node_id: str, heartbeat_ttl: int) -> List[dict]:
    engine = SyncEngine(redis_url, node_id, heartbeat_ttl)
    return engine.step()


def alignment_error_ms(groups: List[dict], true_time: float) -> float:
    if not groups:
        return math.nan
    t_hat = groups[0]["t_global"]
    return abs(t_hat - true_time) * 1000.0


def max_member_probability(groups: List[dict], sensor_id: str) -> float:
    if not groups:
        return 0.0
    members = groups[0]["members"]
    for m in members:
        if m["sensor_id"] == sensor_id:
            return m["probability"]
    return 0.0


def member_probability(groups: List[dict], sensor_id: str) -> float:
    if not groups:
        return 0.0
    for m in groups[0]["members"]:
        if m["sensor_id"] == sensor_id:
            return m["probability"]
    return 0.0


def false_match_probability(groups: List[dict], distractor_id: str) -> float:
    return member_probability(groups, distractor_id)


def nearest_baseline_false(rng: random.Random, jitter: float, true_time: float, delta: float) -> float:
    """Nearest-timestamp baseline false rate for one trial.

    Returns 1.0 if distractor is nearer than the correct sensor to true_time, else 0.0.
    """
    t_correct = true_time + rng.gauss(0.0, jitter)
    t_distractor = true_time + delta + rng.gauss(0.0, jitter)
    return 1.0 if abs(t_distractor - true_time) < abs(t_correct - true_time) else 0.0


def ensure_figures_dir() -> None:
    import pathlib

    pathlib.Path("experiments/figures").mkdir(parents=True, exist_ok=True)


def save_fig(fig, name: str) -> None:
    ensure_figures_dir()
    fig.savefig(f"experiments/figures/{name}.pdf", bbox_inches="tight")
    fig.savefig(f"experiments/figures/{name}.png", dpi=300, bbox_inches="tight")
```}