mod keycode;

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

pub use self::keycode::Keycode;

/// Pinout configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// default debounce time in milliseconds
    default_debounce_ms: u64,
    /// default active_low setting for keys; if true, key is active when GPIO is low
    pub default_active_low: bool,
    /// polling interval in milliseconds
    poll_interval_ms: u64,
    /// Keys configuration
    #[serde(rename = "key", default)]
    pub keys: Vec<KeyConfig>,
    /// Power switches configuration
    #[serde(rename = "powerswitch", default)]
    pub power_switches: Vec<PowerSwitchConfig>,
}

impl AppConfig {
    /// Load configuration from the specified file path
    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {:?}: {}", path, e))?;
        let config: AppConfig = toml::from_str(&config_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse config file {:?}: {}", path, e))?;
        Ok(config)
    }

    /// Default debounce time
    pub fn default_debounce(&self) -> Duration {
        Duration::from_millis(self.default_debounce_ms)
    }

    /// Polling interval
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }
}

/// Configuration for an individual key
#[derive(Debug, Clone, Deserialize)]
pub struct KeyConfig {
    /// GPIO pin number
    pub gpio: u8,
    /// [`Keycode`] to emit
    pub keycode: Keycode,
    debounce_ms: Option<u64>,
    /// Whether the key is active low; if true, key is active when GPIO is low
    pub active_low: Option<bool>,
    /// Whether auto-repeat is enabled
    pub repeat: bool,
    repeat_delay_ms: Option<u64>,
    repeat_rate_ms: Option<u64>,
}

impl KeyConfig {
    /// Debounce time
    pub fn debounce(&self) -> Option<Duration> {
        self.debounce_ms.map(Duration::from_millis)
    }

    /// Delay before auto-repeat starts
    pub fn repeat_delay(&self) -> Option<Duration> {
        self.repeat_delay_ms.map(Duration::from_millis)
    }

    /// Interval between auto-repeats
    pub fn repeat_rate(&self) -> Option<Duration> {
        self.repeat_rate_ms.map(Duration::from_millis)
    }
}

/// Configuration for an individual power switch
#[derive(Debug, Clone, Deserialize)]
pub struct PowerSwitchConfig {
    /// GPIO pin number
    pub gpio: u8,
    /// Whether the switch is active low; if true, switch is active when GPIO is low
    pub active_low: Option<bool>,
}

#[cfg(test)]
mod tests {

    use rboy::KeypadKey;
    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_should_parse_config() {
        let config: AppConfig = toml::from_str(DEFAULT_CONFIG).unwrap();

        assert_eq!(config.default_debounce_ms, 20);
        assert_eq!(config.default_active_low, true);
        assert_eq!(config.poll_interval_ms, 5);

        assert_eq!(config.keys.len(), 2);
        assert_eq!(config.keys[0].gpio, 17);
        assert_eq!(config.keys[0].keycode.keycode(), KeypadKey::A);
        assert_eq!(config.keys[0].active_low, Some(true));
        assert_eq!(config.keys[0].debounce_ms, Some(20));
        assert_eq!(config.keys[0].repeat, false);

        assert_eq!(config.keys[1].gpio, 22);
        assert_eq!(config.keys[1].keycode.keycode(), KeypadKey::Up);
        assert_eq!(config.keys[1].repeat, true);
        assert_eq!(config.keys[1].repeat_delay_ms, Some(300));
        assert_eq!(config.keys[1].repeat_rate_ms, Some(80));

        assert_eq!(config.power_switches.len(), 1);
        assert_eq!(config.power_switches[0].gpio, 27);
        assert_eq!(config.power_switches[0].active_low, Some(false));
    }

    #[test]
    fn test_should_load_from_file() {
        let tempfile = NamedTempFile::new().unwrap();
        std::fs::write(tempfile.path(), DEFAULT_CONFIG).unwrap();

        let config = AppConfig::load_from_file(tempfile.path()).unwrap();
        assert_eq!(config.keys.len(), 2);
        assert_eq!(config.power_switches.len(), 1);
    }

    #[test]
    fn test_should_parse_config_without_arrays() {
        let _config: AppConfig = toml::from_str(CONFIG_WNO_ARRAYS).unwrap();
    }

    const DEFAULT_CONFIG: &str = r#"
default_debounce_ms = 20 # default debounce time in milliseconds
default_active_low = true # default active_low setting for keys; if true, key is active when GPIO is low
poll_interval_ms = 5 # polling interval in milliseconds

[[key]]
gpio = 17
keycode = "A"
active_low = true # `default_active_low` by default
debounce_ms = 20 # `default_debounce_ms` by default
repeat = false # disabled by default

[[key]]
gpio = 22
keycode = "UP"
repeat = true
repeat_delay_ms = 300
repeat_rate_ms = 80

[[powerswitch]]
gpio = 27
active_low = false
    "#;

    const CONFIG_WNO_ARRAYS: &str = r#"
default_debounce_ms = 20 # default debounce time in milliseconds
default_active_low = true # default active_low setting for keys; if true, key is active when GPIO is low
poll_interval_ms = 5 # polling interval in milliseconds
    "#;
}
