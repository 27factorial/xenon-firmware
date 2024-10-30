use esp_println::println;
use log::LevelFilter;

const MAX_LOG_LEVEL: log::LevelFilter = match option_env!("XENON_LOGLEVEL") {
    Some(s) => match s.as_bytes() {
        b"OFF" => log::LevelFilter::Off,
        b"ERROR" => log::LevelFilter::Error,
        b"WARN" => log::LevelFilter::Warn,
        b"INFO" => log::LevelFilter::Info,
        b"DEBUG" => log::LevelFilter::Debug,
        b"TRACE" => log::LevelFilter::Trace,
        _ => panic!("Invalid value set for `XENON_LOGLEVEL` environment variable"),
    },
    None => log::LevelFilter::Info,
};

pub fn init_logger(level: LevelFilter) {
    log::set_max_level(level);
    log::set_logger(&Logger).expect("attempted to initialize logger twice");
}

pub fn init_logger_from_env() {
    log::set_max_level(MAX_LOG_LEVEL);
    log::set_logger(&Logger).expect("attempted to initialize logger twice");
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        const COLOR_RESET: &str = "\u{001B}[0m";
        const COLOR_RED: &str = "\u{001B}[31m";
        const COLOR_GREEN: &str = "\u{001B}[32m";
        const COLOR_YELLOW: &str = "\u{001B}[33m";
        const COLOR_BLUE: &str = "\u{001B}[34m";
        const COLOR_CYAN: &str = "\u{001B}[35m";

        let level = record.level();

        if level <= log::max_level() {
            let level_str = level.as_str();
            let level_color = match level {
                log::Level::Error => COLOR_RED,
                log::Level::Warn => COLOR_YELLOW,
                log::Level::Info => COLOR_GREEN,
                log::Level::Debug => COLOR_BLUE,
                log::Level::Trace => COLOR_CYAN,
            };

            let message = record.args();

            match record.target() {
                "" => println!("{level_color}[{level_str}] - {message}{COLOR_RESET}"),
                s => println!("{level_color}[{level_str} @ {s}] - {message}{COLOR_RESET}"),
            };
        }
    }

    fn flush(&self) {
        todo!()
    }
}
