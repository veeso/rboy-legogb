use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl FromStr for LogLevel {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<LogLevel, Self::Err> {
        match input.to_lowercase().as_str() {
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err("invalid log level"),
        }
    }
}

impl From<LogLevel> for log::LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Error => log::LevelFilter::Error,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Trace => log::LevelFilter::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("error").unwrap(), LogLevel::Error);
        assert_eq!(LogLevel::from_str("warn").unwrap(), LogLevel::Warn);
        assert_eq!(LogLevel::from_str("info").unwrap(), LogLevel::Info);
        assert_eq!(LogLevel::from_str("debug").unwrap(), LogLevel::Debug);
        assert_eq!(LogLevel::from_str("trace").unwrap(), LogLevel::Trace);
        assert!(LogLevel::from_str("invalid").is_err());
    }

    #[test]
    fn test_log_level_to_level_filter() {
        assert_eq!(
            log::LevelFilter::from(LogLevel::Error),
            log::LevelFilter::Error
        );
        assert_eq!(
            log::LevelFilter::from(LogLevel::Warn),
            log::LevelFilter::Warn
        );
        assert_eq!(
            log::LevelFilter::from(LogLevel::Info),
            log::LevelFilter::Info
        );
        assert_eq!(
            log::LevelFilter::from(LogLevel::Debug),
            log::LevelFilter::Debug
        );
        assert_eq!(
            log::LevelFilter::from(LogLevel::Trace),
            log::LevelFilter::Trace
        );
    }
}
