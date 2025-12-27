use std::path::PathBuf;

/// rboy-lego - rboy emulator adapted to run on Raspberry Pi with framebuffer and GPIO input
#[derive(argh::FromArgs, Debug)]
pub struct Args {
    /// bytes per pixel for the framebuffer
    #[argh(option, default = "2")]
    pub bytes_per_pixel: usize,
    /// path to config file
    #[argh(option, default = "PathBuf::from(\"/etc/rboy-lego/config.toml\")")]
    pub config: PathBuf,
    /// path to framebuffer device
    #[argh(option, default = "PathBuf::from(\"/dev/fb0\")")]
    pub framebuffer_path: PathBuf,
    /// framebuffer height
    #[argh(option, default = "240")]
    pub height: usize,
    /// scale factor for the framebuffer output
    #[argh(option, default = "2")]
    pub scale: usize,
    /// framebuffer stride in pixels
    #[argh(option, default = "320")]
    pub stride_pixels: usize,
    /// framebuffer width
    #[argh(option, default = "320")]
    pub width: usize,
    /// path to ROM file
    #[argh(positional)]
    pub rom_path: Option<PathBuf>,
}
