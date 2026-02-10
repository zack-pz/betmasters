mod coordinator;
mod logger;

use coordinator::Coordinator;
use std::process::ExitCode;
use tokio::signal;

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

            eprintln!("Error: {}", err);
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let coordinator = Coordinator::new();
    
    tokio::select! {
        res = coordinator.run(8080) => {
            if let Err(e) = res {
                eprintln!("Coordinator error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            println!("\nShutting down gracefully...");
        }
    }

    Ok(())
}