# BetMasters

A distributed Mandelbrot fractal generator implemented in Rust, designed to run across multiple machines connected via a WireGuard VPN.

---

## Table of Contents

1. [Project Description](#1-project-description)
2. [Network Architecture: WireGuard VPN](#2-network-architecture-wireguard-vpn)
3. [Distributed Architecture: Coordinator & Workers](#3-distributed-architecture-coordinator--workers)
4. [Code Structure](#4-code-structure)
5. [Mandelbrot Algorithm](#5-mandelbrot-algorithm)
6. [Design Decisions](#6-design-decisions)
7. [Running the Project](#7-running-the-project)
8. [Docker](#8-docker)

---

## 1. Project Description

BetMasters computes Mandelbrot fractal images by distributing the workload across multiple machines. The image is divided into horizontal row blocks; each block is sent to a worker process for computation, and the coordinator reassembles the results into a final PNG file.

The system was designed to operate over a **WireGuard VPN** — all nodes (coordinator and workers) share a private encrypted network, and workers can run on any device (laptop, server, cloud VM) as long as they are connected to the VPN. This eliminates the need for public IP addresses or complex service discovery: workers only need to know the coordinator's stable VPN IP.

A single compiled binary handles both roles. The role is selected at runtime via the `APP_ROLE` environment variable, which simplifies deployment: the same binary is deployed to every node, and the role is determined by configuration alone.

---

## 2. Network Architecture: WireGuard VPN

The project runs on a **Hub-and-Spoke** WireGuard topology:

```
                    ┌─────────────────────────────┐
                    │       WireGuard VPN          │
                    │         10.0.0.0/24          │
                    │                              │
        ┌───────────┤  Hub: Coordinator            │
        │           │  10.0.0.1 (Ubuntu server)    │
        │           └──────────────────────────────┘
        │                        │
   wg tunnel                wg tunnel
        │                        │
   ┌────┴────┐             ┌──────┴────┐
   │ Worker  │             │  Worker   │
   │10.0.0.2 │             │ 10.0.0.3  │
   │ Laptop  │             │ AWS EC2   │
   └─────────┘             └───────────┘
```

- The **coordinator** runs on the Hub node, which has a fixed VPN IP (e.g., `10.0.0.1`) and a public UDP port open for WireGuard (`51820`).
- **Workers** are Spokes that connect to the VPN from any device. Once connected, they reach the coordinator at its VPN IP — regardless of physical location or network.
- All traffic is encrypted end-to-end by WireGuard. Workers do not need public IPs or open inbound ports.

The VPN is configured so that Spokes can communicate with the Hub but not directly with each other, which is sufficient for this architecture since all coordination flows through the coordinator.

---

## 3. Distributed Architecture: Coordinator & Workers

### Roles

| Role | Responsibility |
|------|---------------|
| **Coordinator** | Partitions the image into tasks, distributes them to workers, tracks completion, detects timeouts, saves the final PNG. |
| **Worker** | Connects to the coordinator via WebSocket, receives a block of rows to compute, runs the Mandelbrot evaluation, and returns the result. |

### Communication Flow

All coordinator-worker communication happens over **WebSocket** on the `/ws` endpoint.

```
Worker                              Coordinator
  │                                      │
  │──── WS Connect (/ws) ──────────────>│
  │──── Hello ─────────────────────────>│  Worker registered, alias assigned
  │                                      │
  │<─── Compute {task_id, rows} ────────│  Task dispatched
  │                                      │
  │  [computes Mandelbrot for rows]      │
  │                                      │
  │──── ComputeResult {task_id, data} ->│  Data merged into storage buffer
  │                                      │
  │<─── Compute {task_id, rows} ────────│  Next task (if any)
  │        ...                           │
  │                                      │
  │                               [all tasks done]
  │                               save fractal.png
```

### Coordinator Internals

The coordinator's main loop uses `tokio::select!` to concurrently handle two event sources:

- **1-second tick**: checks for tasks that exceeded the 3-second timeout and re-queues them; triggers auto-start if 10 seconds have elapsed since the last worker connection.
- **`InternalMessage` channel**: receives events from Axum WebSocket handlers (`WorkerConnected`, `WorkerDisconnected`, `WorkerFinished`).

This design keeps all mutable coordinator state in a single async task, avoiding shared-memory synchronization.

```
┌──────────────────────────────────────────────────┐
│                Coordinator Loop                   │
│                                                   │
│  tokio::select! {                                 │
│    tick (1s) => timeout check + auto-start        │
│    rx.recv() => WorkerConnected                   │
│               | WorkerDisconnected                │
│               | WorkerFinished                    │
│  }                                                │
└──────────────────────────────────────────────────┘
         ▲                            ▲
         │  InternalMessage (mpsc)    │
┌────────┴────────┐        ┌──────────┴──────────┐
│  ws_handler #1  │        │   ws_handler #2      │
│  (Axum task)    │        │   (Axum task)        │
└─────────────────┘        └─────────────────────┘
```

### Worker Internals

Each worker runs an infinite reconnection loop. On connection:

1. Sends a `Hello` message.
2. Waits for `Compute` commands in a loop.
3. On receiving a `Compute` command, calls `compute_block()` synchronously and sends back `ComputeResult`.
4. If the connection drops, it waits 2 seconds and reconnects automatically.

---

## 4. Code Structure

```
betmasters/
├── .github/
│   └── workflows/
│       └── gatekeeper.yml          # PR gate: main ← development only
├── docker/
│   ├── .env.example
│   └── docker-compose.yml
├── docs/
│   ├── Configuracion Avanzada.md
│   ├── Docker compose.md
│   ├── Hub-and-Spoke.md
│   └── Virtual private network.md
├── kubernetes/
├── vpn/
└── rust/
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # Entry point; reads APP_ROLE, builds Coordinator or Worker
        ├── logger.rs               # Logging initialization
        ├── coordinator/
        │   ├── logic.rs            # Coordinator struct, task queue, select! loop, image save
        │   ├── handlers.rs         # Axum HTTP + WebSocket handlers
        │   └── types.rs            # CoordinatorCommand, WorkerMessage, InternalMessage, Task, WorkerState
        └── worker/
            ├── logic.rs            # Worker::run(), WebSocket session, reconnect loop
            ├── math.rs             # compute_block(), evaluate() — pure Mandelbrot computation
            └── types.rs            # Worker struct definition
```

### Module Responsibilities

**`main.rs`** — reads environment variables and constructs either a `Coordinator` or a `Worker`. No business logic lives here; it is purely a configuration and dispatch layer.

**`coordinator/logic.rs`** — owns all mutable coordinator state: the pending task queue (`VecDeque<Task>`), assigned tasks (`HashMap<u32, Task>`), worker registry (`HashMap<String, WorkerState>`), idle worker queue, and the pixel storage buffer (`Vec<u32>`). Contains the `tokio::select!` event loop and the `save_image_impl` / `iter_to_color` functions for PNG output.

**`coordinator/handlers.rs`** — Axum WebSocket upgrade handler. Each connection spawns two concurrent tasks: `forward_commands` (sends `CoordinatorCommand` JSON frames to the worker) and `receive_results` (reads `WorkerMessage` frames and forwards them as `InternalMessage` to the coordinator loop via mpsc).

**`coordinator/types.rs`** — all message types serialized over WebSocket (`CoordinatorCommand`, `WorkerMessage`) and the internal channel type (`InternalMessage`). Also defines `Task` and `WorkerState`.

**`worker/logic.rs`** — implements `Worker::run()` with the reconnection loop and `run_session()` for the active WebSocket session.

**`worker/math.rs`** — pure computation: maps pixel coordinates to complex plane coordinates, evaluates the Mandelbrot iteration for each point, and returns iteration counts as `Vec<u32>`.

**`worker/types.rs`** — the `Worker` struct holding configuration (`coordinator_url`, `max_iters`, complex plane bounds).

---

## 5. Mandelbrot Algorithm

### Mathematical Foundation

The Mandelbrot set is defined over the complex plane. For each point `c = x + yi`, we iterate the recurrence:

```
z₀ = 0
z_{n+1} = z_n² + c
```

A point `c` is considered **outside** the set (it escapes) if `|z_n| > 2` for some finite `n`. Since `|z|² = a² + b²` (where `a = Re(z)`, `b = Im(z)`), the escape condition is checked as `a² + b² > 4.0` — avoiding a square root at every iteration.

If a point reaches `MAX_ITERS` iterations without escaping, it is considered **inside** the set and rendered black.

### Implementation (`math.rs`)

For each pixel at screen coordinates `(col, row)`, the complex coordinate is:

```
c_x = x_min + col * (x_max - x_min) / width
c_y = y_min + row * (y_max - y_min) / height
```

The iteration expands `z² + c` algebraically (avoiding complex number overhead):

```
Re(z² + c) = a² - b² + c_x
Im(z² + c) = 2ab   + c_y
```

The function returns the iteration count `i` at which `a² + b² > 4`, or `MAX_ITERS` if the point did not escape.

### Block Partitioning

The coordinator partitions the image into horizontal bands of `BLOCK_SIZE` rows each at startup:

```
Image (height = H rows)
├── Task 0: rows   0 ..  99
├── Task 1: rows 100 .. 199
├── Task 2: rows 200 .. 299
│   ...
└── Task N: rows (H - remainder) .. H
```

Each task is sent to one worker. Workers compute all pixels in their assigned rows and return a flat `Vec<u32>` of iteration counts. The coordinator writes the result directly into the correct offset of the storage buffer.

### Colorization: Bernstein Polynomials

Once all tasks are complete, the coordinator maps each iteration count to an RGB color using Bernstein polynomial basis functions. Let `t = iter / MAX_ITERS`:

```
R = 9.0  × (1-t) × t³  × 255
G = 15.0 × (1-t)² × t² × 255
B = 8.5  × (1-t)³ × t  × 255
```

This produces a smooth gradient from dark blue/green (slow escape) to bright orange/yellow (fast escape), with black for interior points (`iter == MAX_ITERS`). The polynomial form guarantees continuity and avoids hard color band boundaries.

---

## 6. Design Decisions

### Single binary with `APP_ROLE`

A single Rust binary compiles both the coordinator and worker. The role is selected at runtime via the `APP_ROLE` environment variable (`coordinator` | `worker`).

**Why:** In a VPN deployment, the same binary is distributed to every node. Changing a node's role requires only an environment variable change — no recompilation, no separate artifacts. This is especially practical with Docker: one image, different env vars.

**Trade-off:** Both roles share the same dependency tree, increasing binary size slightly.

### WebSocket over HTTP polling

Workers connect to the coordinator's `/ws` endpoint and maintain a persistent WebSocket connection. Task dispatch and result delivery both happen over this single connection.

**Why:** With workers distributed across a WireGuard VPN, each worker only needs to know one URL: the coordinator's VPN IP. The WebSocket connection is persistent and bidirectional — the coordinator can push tasks to the worker as soon as they become available, without polling overhead. This also maps naturally to the Hub-and-Spoke topology: all traffic flows through the coordinator node.

**Trade-off:** WebSocket requires the coordinator to maintain one open connection per worker. For the scale of this project (tens of nodes) this is negligible.

### `mpsc` channels between handlers and coordinator loop

Axum WebSocket handlers run as independent async tasks. Rather than giving them direct access to the coordinator's mutable state (which would require `Arc<Mutex<Coordinator>>`), each handler sends events to the coordinator loop through a Tokio `mpsc` channel.

**Why:** The coordinator's state (task queue, worker registry, storage buffer) is modified in many places and in a specific order. An `Arc<Mutex<>>` would work but introduces lock contention and the risk of deadlocks. With `mpsc`, all state mutations happen in a single async task in a well-defined sequence, which is simpler to reason about and debug.

**Trade-off:** The coordinator loop becomes a bottleneck for state mutations. For this workload (one message per task completion), that is not a concern.

### `spawn_blocking` for image saving

When all tasks are complete, the coordinator calls `save_image_impl` via `tokio::task::spawn_blocking`.

**Why:** Encoding a 3840×2160 PNG involves significant CPU work (pixel iteration, PNG compression) and blocking I/O. Running this on the Tokio async runtime directly would block the event loop, preventing the coordinator from handling any other messages. `spawn_blocking` offloads it to a dedicated thread pool designed for blocking operations.

---

## 7. Running the Project

All commands from the `rust/` directory.

### Build

```bash
cargo build
cargo build --release
```

### Run as Coordinator

```bash
APP_ROLE=coordinator \
PORT=8080 \
IMAGE_WIDTH=3840 \
IMAGE_HEIGHT=2160 \
BLOCK_SIZE=100 \
MAX_ITERS=1000 \
cargo run
```

### Run as Worker

```bash
APP_ROLE=worker \
PORT=8081 \
COORDINATOR_URL=http://10.0.0.1:8080 \
MAX_ITERS=1000 \
X_MIN=-2.0 \
X_MAX=1.0 \
Y_MIN=-1.5 \
Y_MAX=1.5 \
cargo run
```

Replace `10.0.0.1` with the coordinator's WireGuard VPN IP. Workers will automatically reconnect if the connection drops.

### Environment Variables

| Variable | Role | Default | Description |
|----------|------|---------|-------------|
| `APP_ROLE` | both | `coordinator` | `coordinator` or `worker` |
| `PORT` | both | `8080` | Listening port (coordinator) or informational (worker) |
| `COORDINATOR_URL` | worker | `http://127.0.0.1:8080` | Full HTTP URL of the coordinator |
| `IMAGE_WIDTH` | coordinator | `3840` | Output image width in pixels |
| `IMAGE_HEIGHT` | coordinator | `2160` | Output image height in pixels |
| `BLOCK_SIZE` | coordinator | `100` | Number of rows per task |
| `MAX_ITERS` | both | `1000` | Maximum Mandelbrot iterations |
| `X_MIN` / `X_MAX` | worker | `-2.0` / `1.0` | Complex plane horizontal bounds |
| `Y_MIN` / `Y_MAX` | worker | `-1.5` / `1.5` | Complex plane vertical bounds |
| `OUTPUT_FILE` | coordinator | `fractal.png` | Output PNG file path |
| `BIND_ADDR` | coordinator | `0.0.0.0` | Bind address for the HTTP server |

### Run Tests

```bash
cargo test

# Run a single test
cargo test <test_name>
```

---

## 8. Docker

From the `docker/` directory:

```bash
cp .env.example .env
# Edit .env as needed
docker compose up --build
```

---

## Git Workflow

PRs to `main` must originate from the `development` branch, enforced by `.github/workflows/gatekeeper.yml`.
