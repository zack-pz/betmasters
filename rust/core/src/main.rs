mod coordinator;
mod logger;
mod worker;

use clap::{Parser, ValueEnum};
use coordinator::Coordinator;
use std::process::ExitCode;
use tokio::signal;
use worker::Worker;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Type of the node to run
    #[arg(short, long, value_enum, default_value_t = NodeType::Coordinator)]
    type_: NodeType,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum NodeType {
    Coordinator,
    Worker,
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
        match args.type_ {
            NodeType::Coordinator => {
                let coordinator = Coordinator::new();
                if let Err(e) = coordinator.run(8080).await {
                    eprintln!("Coordinator error: {}", e);
                }
            }
            NodeType::Worker => {
                let worker = Worker::new("ws://127.0.0.1:8080/ws".to_string());
                if let Err(e) = worker.run().await {
                    eprintln!("Worker error: {}", e);
                }
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
