# Sensorium

**Probabilistic Sensor Synchronization Library**

A Rust library with Python bindings for synchronizing multi-sensor observations using probabilistic time offset modeling and Redis-based coordination.

## Features

- **Sensor-agnostic**: Works with any sensor type (cameras, IMUs, microphones, etc.)
- **Probabilistic matching**: No hard thresholds; uses Gaussian PDFs for association
- **Distributed coordination**: Redis as the only communication layer
- **Leader election**: Bully algorithm for master/follower coordination
- **Kalman filtering**: Adaptive time offset estimation per sensor
- **Python-friendly**: Clean API via maturin/pyo3 bindings

## Architecture

```
sensorium/
├── crates/
│   ├── sensor-core/      # Core traits, observation structs, Gaussian PDF
│   ├── sensor-redis/     # Redis I/O, key builders, data structures
│   ├── sensor-sync/      # Time offset modeling, probabilistic grouping
│   ├── sensor-election/  # Bully leader election via Redis heartbeats
│   └── sensor-python/    # Python bindings (pyo3/maturin)
├── examples/             # Python examples and CLI tools
└── tests/                # pytest integration tests
```

## Quick Start

### Prerequisites

- Rust (>= 1.75)
- Python (>= 3.8)
- Redis server running on `localhost:6379`
- maturin for Python bindings

### Build & Install

```bash
# Clone repository
git clone https://github.com/afeldman/sensorium.git
cd sensorium

# Build Rust workspace
cargo build --release

# Run Rust tests
cargo test --workspace

# Build and install Python bindings
maturin develop

# Run Python integration tests (requires Redis)
pytest tests/ -v
```

### Python Usage

```python
from sensorium import SyncEngine

# Initialize sync engine
engine = SyncEngine(
    redis_url="redis://127.0.0.1/",
    node_id="node-1",
    heartbeat_ttl=5
)

# Perform synchronization step
groups = engine.step()

# Process synchronized groups
for group in groups:
    print(f"Global time: {group['t_global']:.6f}s")
    for member in group['members']:
        print(f"  {member['sensor_id']}: P={member['probability']:.4f}")
```

### Example: Synthetic Sensors

```bash
# Run synthetic sensor simulation
python examples/synthetic_sensors.py
```

### Example: Minimal deterministic experiment

```bash
python examples/minimal_experiment.py --redis-url redis://127.0.0.1/ --node-id node-a --seed 42
```

Returns a single time slice with normalized membership probabilities. Deterministic for fixed seeds.

### CLI Tool

```bash
# Ingest a sensor observation
python examples/cli.py ingest camera-1 camera 10.5 --sigma 0.01

# Synchronize observations
python examples/cli.py sync
```

## How It Works

1. **Time Offset Modeling**: Each sensor has a `TimeOffsetModel` (offset, variance, drift) mapping local to global time
2. **Kalman Updates**: Time offsets are refined with each new observation
3. **Probabilistic Association**: Observations are matched using Gaussian PDFs over time residuals
4. **Soft Clustering**: Groups are formed via precision-weighted averaging; no hard thresholds
5. **Leader Election**: Only the master node (highest `node_id`) writes synchronized groups to Redis

## Getting Started

To get started, clone the repository and build the project:

```sh
git clone https://github.com/afeldman/sensorium.git
cd sensorium
cargo build
```

## Documentation

To generate the documentation, run:

```sh
cargo doc --workspace --open
```
