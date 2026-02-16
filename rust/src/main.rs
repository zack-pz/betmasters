mod coordinator;
mod logger;
mod worker;

use clap::Parser;
use coordinator::Coordinator;
use std::process::ExitCode;
use tokio::signal;
use worker::Worker;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Run as coordinator
    #[arg(short = 'c', long)]
    coordinator: bool,

    /// Run as worker
    #[arg(short = 'w', long)]
    worker: bool,
}

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
    let args = Args::parse();

    let task = async move {
        if args.worker {
            let worker = Worker::<f64>::new("http://127.0.0.1:8080".to_string());
            if let Err(e) = worker.run().await {
                eprintln!("Worker error: {}", e);
            }
        } else {
            // Default to coordinator if --worker is not specified, 
            // even if --coordinator is not explicitly passed.
            let coordinator = Coordinator::new(100, 100);
            if let Err(e) = coordinator.run(8080).await {
                eprintln!("Coordinator error: {}", e);
            }
        }
    };

    tokio::select! {
        _ = task => {}
        _ = signal::ctrl_c() => {
            println!("\nShutting down gracefully...");
        }
    }

    Ok(())
}
