mod coordinator;
mod logger;
mod worker;

use coordinator::Coordinator;
use log::error;
use std::env;
use std::process::ExitCode;
use worker::Worker;

#[tokio::main]
async fn main() -> ExitCode {
    logger::init();

    match run().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            let mut current: Option<&(dyn std::error::Error + 'static)> = Some(err.as_ref());
            while let Some(cause) = current {
                if let Some(ioerr) = cause.downcast_ref::<std::io::Error>() {
                    if ioerr.kind() == std::io::ErrorKind::BrokenPipe {
                        return ExitCode::SUCCESS;
                    }
                }
                current = cause.source();
            }

            error!("Error: {}", err);
            ExitCode::from(2)
        }
    }
}

fn env_string(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let role = env_string("APP_ROLE", "coordinator");
    let bind_addr = env_string("BIND_ADDR", "0.0.0.0");
    let port = parse_env("PORT", 8080u16);
    let coordinator_url = env_string("COORDINATOR_URL", "http://127.0.0.1:8080");

    if role == "worker" {
        let max_iters = parse_env("MAX_ITERS", 1000usize);
        let x_min = parse_env("X_MIN", -2.0f64);
        let x_max = parse_env("X_MAX", 1.0f64);
        let y_min = parse_env("Y_MIN", -1.5f64);
        let y_max = parse_env("Y_MAX", 1.5f64);

        let worker = Worker::new(coordinator_url, max_iters, x_min, x_max, y_min, y_max);
        if let Err(e) = worker.run().await {
            error!("Worker error: {}", e);
        }
    } else {
        let width = parse_env("IMAGE_WIDTH", 3840u32);
        let height = parse_env("IMAGE_HEIGHT", 2160u32);
        let block_size = parse_env("BLOCK_SIZE", 100u32);
        let max_iters = parse_env("MAX_ITERS", 1000u32);

        let coordinator = Coordinator::new(width, height, block_size, max_iters);
        if let Err(e) = coordinator.run(&bind_addr, port).await {
            error!("Coordinator error: {}", e);
        }
    }

    Ok(())
}
