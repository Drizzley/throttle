use env_logger;
use failure::{Error, ResultExt};
use gelf;
use log;
use serde::Deserialize;

/// Controls logging behaviour of throttle. Set via the configuration file
#[derive(Deserialize, Default, PartialEq, Eq, Clone, Debug)]
pub struct LoggingConfig {
    /// Configures a Gelf Logger
    pub gelf: Option<GelfConfig>,
}

#[derive(Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct GelfConfig {
    /// Name of the instance. Appears as source in Graylog
    name: String,
    /// Host of e.g. Graylog instance
    host: String,
    /// E.g. "INFO" or "DEBUG"
    level: log::LevelFilter,
    /// E.g. "12201"
    port: u16,
}

/// Initialize GELF logger if `logging_config.json` is found in the working directory.
pub fn init(config: &LoggingConfig) -> Result<(), Error> {
    if let Some(ref config) = config.gelf {
        let backend = gelf::UdpBackend::new(format!("{}:{}", config.host, config.port))
            .context("Error creating GELF UDP logging backend")?;
        let mut logger =
            gelf::Logger::new(Box::new(backend)).context("Error creating GELF logger.")?;
        logger.set_hostname(config.name.as_str());
        logger
            .install(config.level)
            .context("Failed to install logger")?;
    } else {
        eprintln!("gelf logger config not found => Logging to stderr instead");
        env_logger::init();
    }
    Ok(())
}

