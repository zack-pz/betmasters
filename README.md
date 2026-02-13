# BetMasters

A distributed Mandelbrot fractal generator implemented in Rust.

## Project Structure

```text
/home/zack/work/betmasters/
├───.github/
│   └───workflows/
│       └───gatekeeper.yml
├───docker/
│   ├───.env.example
│   └───docker-compose.yml
├───docs/
│   ├───Configuracion Avanzada.md
│   ├───Docker compose.md
│   ├───Hub-and-Spoke.md
│   └───Virtual private network.md
├───kubernetes/
├───rust/
│   ├───Cargo.toml
│   ├───src/
│   │   ├───coordinator.rs
│   │   ├───core.rs
│   │   ├───lib.rs
│   │   ├───logger.rs
│   │   ├───main.rs
│   │   ├───worker.rs
│   │   ├───coordinator/
│   │   │   ├───handlers.rs
│   │   │   ├───logic.rs
│   │   │   └───types.rs
│   │   └───worker/
│   │       ├───logic.rs
│   │       ├───math.rs
│   │       └───types.rs
└───vpn/
```

## Execution Commands

To run the program, use the following commands (ensure you are in the `rust/` directory):

### Worker
```bash
cargo run --features rayon -- --type worker
```

### Coordinator
```bash
cargo run --features rayon -- --type coordinator
```
