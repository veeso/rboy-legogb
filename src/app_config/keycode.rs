use std::fmt;
use std::str::FromStr;

use rboy::KeypadKey;

/// Wrapper around [`KeypadKey`] to facilitate deserialization
#[derive(Debug, Clone, Copy)]
pub struct Keycode(KeypadKey);

impl Keycode {
    /// Get the underlying [`KeypadKey`]
    pub fn keycode(&self) -> KeypadKey {
        self.0
    }
}

impl fmt::Display for Keycode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl FromStr for Keycode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "A" => Ok(Keycode(KeypadKey::A)),
            "B" => Ok(Keycode(KeypadKey::B)),
            "UP" => Ok(Keycode(KeypadKey::Up)),
            "DOWN" => Ok(Keycode(KeypadKey::Down)),
            "LEFT" => Ok(Keycode(KeypadKey::Left)),
            "RIGHT" => Ok(Keycode(KeypadKey::Right)),
            "START" => Ok(Keycode(KeypadKey::Start)),
            "SELECT" => Ok(Keycode(KeypadKey::Select)),
            _ => Err("Unsupported keycode"),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Keycode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Keycode::from_str(&s).map_err(serde::de::Error::custom)
    }
}
