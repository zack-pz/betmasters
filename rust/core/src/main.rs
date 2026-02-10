use std::process::ExitCode;
use tokio::signal;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            // We check if the error is a BrokenPipe error
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
    println!("hello world");

    // Wait for Ctrl+C signal for graceful shutdown
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\nShutting down gracefully...");
        }
        Err(err) => {
            eprintln!("Error listening to signal: {}", err);
        }
    }

    Ok(())
}
