/*!
Defines a super simple logger that works with the `log` crate.

We don't do anything fancy. We just need basic log levels and the ability to
print to stderr. We therefore avoid bringing in extra dependencies just for
this functionality.
*/

use log::{LevelFilter, Log};

/// The simplest possible logger that logs to stderr.
#[derive(Debug)]
pub struct Logger(());

/// A singleton used as the target for an implementation of the `Log` trait.
const LOGGER: &'static Logger = &Logger(());

/// Create a new logger that logs to stderr and initialize it as the
/// global logger.
pub fn init() {
    log::set_logger(LOGGER).expect("Failed to set logger");
    log::set_max_level(LevelFilter::Info);
}

impl Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            match (record.file(), record.line()) {
                (Some(file), Some(line)) => {
                    eprintln!(
                        "{}|{}|{}:{}: {}",
                        record.level(),
                        record.target(),
                        file,
                        line,
                        record.args()
                    );
                }
                (Some(file), None) => {
                    eprintln!(
                        "{}|{}|{}: {}",
                        record.level(),
                        record.target(),
                        file,
                        record.args()
                    );
                }
                _ => {
                    eprintln!(
                        "{}|{}: {}",
                        record.level(),
                        record.target(),
                        record.args()
                    );
                }
            }
        }
    }

    fn flush(&self) {}
}