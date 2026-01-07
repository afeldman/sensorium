"""Figure 3 – False Association Rate vs. Jitter.

Scenario: one correct sensor near true time, one distractor 300 ms away.
False association = probability mass assigned to the distractor.
Baseline = nearest timestamp heuristic over Monte Carlo trials (seeded).

Outputs: experiments/figures/fig_false_association.pdf/png
"""
import matplotlib.pyplot as plt
import numpy as np
import random

from common import (
    ExperimentConfig,
    SensorSpec,
    false_match_probability,
    flush_db,
    nearest_baseline_false,
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
    true_time = 10.0
    delta = 0.3  # 300 ms distractor offset
    base_seed = 777
    jitters_ms = np.linspace(1, 80, 12)
    trials = 50

    prob_false = []
    prob_baseline = []

    for i, jm in enumerate(jitters_ms):
        jitter = jm / 1000.0
        rng = random.Random(base_seed + i)

        # Sensorium probability
        r = flush_db(cfg.redis_url)
        sensors = [
            SensorSpec(sensor_id="correct", sensor_type="test", offset=0.0, drift=1.0, jitter=jitter),
            SensorSpec(sensor_id="distractor", sensor_type="test", offset=delta, drift=1.0, jitter=jitter),
        ]
        seed_observations(r, sensors, true_time, rng, ttl_seconds=cfg.ttl_seconds)
        groups = run_sync(cfg.redis_url, node_id="node-false", heartbeat_ttl=cfg.heartbeat_ttl)
        prob_false.append(false_match_probability(groups, "distractor") * 100.0)

        # Baseline nearest timestamp across trials (deterministic RNG sequence)
        baseline_sum = 0.0
        for t in range(trials):
            baseline_sum += nearest_baseline_false(random.Random(base_seed + i * 1000 + t), jitter, true_time, delta)
        prob_baseline.append((baseline_sum / trials) * 100.0)

    fig, ax = plt.subplots(figsize=(4, 3))
    ax.plot(jitters_ms, prob_false, marker="o", label="Sensorium (probabilistic)")
    ax.plot(jitters_ms, prob_baseline, marker="s", label="Nearest timestamp baseline")
    ax.set_xlabel("Jitter std dev (ms)")
    ax.set_ylabel("False association rate (%)")
    ax.set_title("False Association vs. Jitter")
    ax.grid(True, linestyle="--", linewidth=0.6, alpha=0.6)
    ax.set_ylim(0, 100)
    ax.legend()
    fig.tight_layout()

    save_fig(fig, "fig_false_association")


if __name__ == "__main__":
    main()
