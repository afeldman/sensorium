"""Figure 4 – Master Failover Recovery.

Simulates a master failure by deleting its heartbeat key and switching to a
new master. Tracks alignment error over time.

Outputs: experiments/figures/fig_failover.pdf/png
"""
import matplotlib.pyplot as plt
import numpy as np
import random
import redis

from common import (
    ExperimentConfig,
    SensorSpec,
    alignment_error_ms,
    flush_db,
    run_sync,
    seed_observations,
    save_fig,
)

plt.rcParams.update({
    "font.size": 10,
    "axes.labelsize": 10,
    "axes.titlesize": 11,
    "legend.fontsize": 9,
    "lines.linewidth": 1.6,
})


def simulate_failover(cfg: ExperimentConfig, steps: int, fail_step: int, seed: int):
    true_time_base = 10.0
    rng = random.Random(seed)
    errors = []
    times = []

    r = flush_db(cfg.redis_url)
    # Warm-up sensors (moderate jitter)
    sensors = [
        SensorSpec(sensor_id="cam", sensor_type="camera", offset=0.02, drift=1.0001, jitter=0.01),
        SensorSpec(sensor_id="imu", sensor_type="imu", offset=-0.01, drift=0.9999, jitter=0.02),
    ]

    for step in range(steps):
        true_time = true_time_base + step * 0.2
        seed_observations(r, sensors, true_time, rng, ttl_seconds=cfg.ttl_seconds)

        # Master A before failure, Master B after
        if step < fail_step:
            groups = run_sync(cfg.redis_url, node_id="node-a", heartbeat_ttl=cfg.heartbeat_ttl)
        else:
            # Simulate heartbeat loss of node-a
            redis.Redis.from_url(cfg.redis_url, decode_responses=True).delete("election:bully:hb:node-a")
            groups = run_sync(cfg.redis_url, node_id="node-b", heartbeat_ttl=cfg.heartbeat_ttl)

        errors.append(alignment_error_ms(groups, true_time))
        times.append(step * 0.2)

    return np.array(times), np.array(errors)


def main() -> None:
    cfg = ExperimentConfig()
    times, errors = simulate_failover(cfg, steps=15, fail_step=7, seed=2024)

    fig, ax = plt.subplots(figsize=(4, 3))
    ax.plot(times, errors, marker="o", label="Alignment error")
    ax.axvline(times[7], color="k", linestyle="--", linewidth=1.0, label="Master failure")
    ax.set_xlabel("Time (s)")
    ax.set_ylabel("Alignment error (ms)")
    ax.set_title("Master Failover Recovery")
    ax.grid(True, linestyle="--", linewidth=0.6, alpha=0.6)
    ax.legend()
    fig.tight_layout()

    save_fig(fig, "fig_failover")


if __name__ == "__main__":
    main()
