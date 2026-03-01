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

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let role = env::var("APP_ROLE").unwrap_or_else(|_| "coordinator".to_string());
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port_env = env::var("PORT").ok().and_then(|s| s.parse::<u16>().ok());

    let coordinator_url =
        env::var("COORDINATOR_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    if role == "worker" {
        let worker = Worker::new(coordinator_url);
        if let Err(e) = worker.run().await {
            error!("Worker error: {}", e);
        }
    } else {
        let port = port_env.unwrap_or(8080);
        let coordinator = Coordinator::new(3840, 2160);
        if let Err(e) = coordinator.run(&bind_addr, port).await {
            error!("Coordinator error: {}", e);
        }
    }

    Ok(())
}
