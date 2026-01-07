# Sensorium

Sensorium is a Rust workspace for sensor data processing and synchronization. It is composed of several crates, each with a specific responsibility.

## Crates

- `sensor-core`: Provides the core data structures and functions for sensor data processing.
- `sensor-election`: A placeholder for leader election logic.
- `sensor-python`: Provides Python bindings for the `SyncEngine`.
- `sensor-redis`: Defines the data structures that are stored in Redis.
- `sensor-sync`: A placeholder for time synchronization logic.

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
