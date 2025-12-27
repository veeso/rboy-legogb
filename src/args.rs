mod log_level;

use std::path::PathBuf;

pub use self::log_level::LogLevel;

/// rboy-lego - rboy emulator adapted to run on Raspberry Pi with framebuffer and GPIO input
#[derive(argh::FromArgs, Debug)]
pub struct Args {
    /// bytes per pixel for the framebuffer (default: 2)
    #[argh(option, default = "2")]
    pub bytes_per_pixel: usize,
    /// path to config file (default: /etc/rboy-lego/config.toml)
    #[argh(option, default = "PathBuf::from(\"/etc/rboy-lego/config.toml\")")]
    pub config: PathBuf,
    /// path to framebuffer device (default: /dev/fb1)
    #[argh(option, default = "PathBuf::from(\"/dev/fb1\")")]
    pub framebuffer_path: PathBuf,
    /// framebuffer height (default: 240)
    #[argh(option, default = "240")]
    pub height: usize,
    /// log level (error, warn, info, debug, trace) (default: info)
    #[argh(option, default = "log_level::LogLevel::Info")]
    pub log_level: log_level::LogLevel,
    /// scale factor for the framebuffer output (default: 2)
    #[argh(option, default = "2")]
    pub scale: usize,
    /// framebuffer stride in pixels (default: 320)
    #[argh(option, default = "320")]
    pub stride_pixels: usize,
    /// framebuffer width (default: 320)
    #[argh(option, default = "320")]
    pub width: usize,
    /// path to ROM file
    #[argh(positional)]
    pub rom_path: Option<PathBuf>,
}
