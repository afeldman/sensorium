"""Figure 2 – Probability Mass vs. Jitter.

Tracks the probability assigned to the correct sensor while jitter grows.
Uses a distractor sensor offset by +50 ms to show confidence degradation.

Outputs: experiments/figures/fig_probability_mass.pdf/png
"""
import matplotlib.pyplot as plt
import numpy as np
import random

from common import (
    ExperimentConfig,
    SensorSpec,
    flush_db,
    max_member_probability,
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


def main() -> None:
    cfg = ExperimentConfig()
    base_seed = 123
    true_time = 10.0
    jitters_ms = np.linspace(1, 50, 10)  # 1 ms to 50 ms

    probs = []
    for i, jm in enumerate(jitters_ms):
        jitter = jm / 1000.0
        rng = random.Random(base_seed + i)
        r = flush_db(cfg.redis_url)
        sensors = [
            SensorSpec(sensor_id="correct", sensor_type="test", offset=0.0, drift=1.0, jitter=jitter),
            SensorSpec(sensor_id="distractor", sensor_type="test", offset=0.05, drift=1.0, jitter=jitter),
        ]
        seed_observations(r, sensors, true_time, rng, ttl_seconds=cfg.ttl_seconds)
        groups = run_sync(cfg.redis_url, node_id="node-prob", heartbeat_ttl=cfg.heartbeat_ttl)
        probs.append(max_member_probability(groups, sensor_id="correct"))

    fig, ax = plt.subplots(figsize=(4, 3))
    ax.plot(jitters_ms, probs, marker="o", label="correct sensor")
    ax.set_xlabel("Jitter std dev (ms)")
    ax.set_ylabel("Top association probability")
    ax.set_title("Probability Mass vs. Jitter")
    ax.grid(True, linestyle="--", linewidth=0.6, alpha=0.6)
    ax.set_ylim(0, 1.05)
    ax.legend()
    fig.tight_layout()

    save_fig(fig, "fig_probability_mass")


if __name__ == "__main__":
    main()
