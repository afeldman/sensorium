"""Figure 1 – Alignment Error vs. Drift.

Uses SyncEngine with deterministic synthetic sensors. Evaluates mean alignment
error for two sensor types (low and high jitter) across increasing drift.

Outputs: experiments/figures/fig_alignment_error.pdf/png
"""
import matplotlib.pyplot as plt
import numpy as np
import random

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


def main() -> None:
    cfg = ExperimentConfig()
    base_seed = 42
    true_time = 10.0
    drifts_ppm = np.array([0, 25, 50, 100, 200], dtype=float)  # parts per million
    drift_factors = 1.0 + drifts_ppm * 1e-6

    curves = [
        ("low-jitter", 0.01),
        ("high-jitter", 0.05),
    ]

    fig, ax = plt.subplots(figsize=(4, 3))

    for idx, (label, jitter) in enumerate(curves):
        errs = []
        for i, drift in enumerate(drift_factors):
            rng = random.Random(base_seed + idx * 100 + i)
            r = flush_db(cfg.redis_url)
            sensors = [
                SensorSpec(sensor_id="s", sensor_type="test", offset=0.0, drift=drift, jitter=jitter),
            ]
            seed_observations(r, sensors, true_time, rng, ttl_seconds=cfg.ttl_seconds)
            groups = run_sync(cfg.redis_url, node_id="node-align", heartbeat_ttl=cfg.heartbeat_ttl)
            errs.append(alignment_error_ms(groups, true_time))
        ax.plot(drifts_ppm, errs, marker="o" if idx == 0 else "s", label=label)

    ax.set_xlabel("Clock drift (ppm)")
    ax.set_ylabel("Mean alignment error (ms)")
    ax.set_title("Alignment Error vs. Drift")
    ax.grid(True, linestyle="--", linewidth=0.6, alpha=0.6)
    ax.legend()
    fig.tight_layout()

    save_fig(fig, "fig_alignment_error")


if __name__ == "__main__":
    main()
