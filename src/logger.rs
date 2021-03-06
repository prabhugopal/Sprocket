use log::{LogRecord, LogLevel, LogMetadata, Log};
use log::LogLevelFilter;
use log::{SetLoggerError, ShutdownLoggerError};
use log;

use console;
use core::fmt::Write;

const RED: &'static str = "41m";
const YELLOW: &'static str = "43m";
const CYAN: &'static str = "44m";
const WHITE: &'static str = "47m";
const GREEN: &'static str = "42m";

const TEXT: &'static str = "\x1b[30m";

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &LogMetadata) -> bool {
        metadata.level() <= LogLevel::Debug
    }

    fn log(&self, record: &LogRecord) {
        if self.enabled(record.metadata()) {
            const ESC: &'static str = "\x1b[";
            const RESET: &'static str = "\x1b[0m";
            let (letter, color_code) = match record.level() {
                LogLevel::Error => ('E', RED),
                LogLevel::Warn => ('W', YELLOW),
                LogLevel::Info => ('I', GREEN),
                LogLevel::Debug => ('D', CYAN),
                LogLevel::Trace => ('T', WHITE),
            };

            let loc = record.location().module_path();

            let idx = match loc.rfind("::") {
                Some(i) => i + 2,
                None => 0,
            };

            // Acquire write lock across the print statements so we atomically
            // write the entire message
            let out = &mut console::CONSOLE.lock();

            write!(out, "{}{}{} {} {}", ESC, color_code, TEXT, letter, RESET).unwrap();
            write!(out, " {:<10} ", &loc[idx..]).unwrap();
            writeln!(out, "{}", record.args()).unwrap();
        }
    }
}


#[allow(dead_code)]
pub fn init() -> Result<(), SetLoggerError> {
    unsafe {
        log::set_logger_raw(|max_level| {
            static LOGGER: SimpleLogger = SimpleLogger;
            max_level.set(LogLevelFilter::Trace);
            &SimpleLogger
        })
    }
}
pub fn shutdown() -> Result<(), ShutdownLoggerError> {
    // if our logger had buffering, this is where we'd flush everything
    Ok(())
}
