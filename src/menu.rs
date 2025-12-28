use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use font8x8::{BASIC_FONTS, UnicodeFonts};
use rboy::KeypadKey;
use rboy::framebuffer::Framebuffer;
use rboy::input::KeyEvent;

use crate::AppState;
use crate::app_config::AppConfig;

const LINE_H: usize = 16;
const PADDING_Y: usize = 16;
const PADDING_X: usize = 16;
const SPACE_SIZE: usize = 8;
const SUBTITLE: &str = "Press start to play a game";
const NO_GAMES: &str = "You have no games in your ROMs directory";

const GAMEBOY_SPLASH_COLOR_RED: u8 = 0xc4;
const GAMEBOY_SPLASH_COLOR_GREEN: u8 = 0xcf;
const GAMEBOY_SPLASH_COLOR_BLUE: u8 = 0xa1;
const SPLASH_TEXT: &str = "Nintendo";
const COLOR_BLACK: u16 = 0x0000;
const COLOR_WHITE: u16 = 0xffff;

/// Gameboy boot sound bytes
const GB_BOOT_SOUND: &[u8] = include_bytes!("../assets/gb_boot.wav");
const GB_BOOT_FILEPATH: &str = "/tmp/gb_boot.wav";

pub struct AppMenu {
    config: Rc<AppConfig>,
    framebuffer: Rc<Framebuffer>,
    event_receiver: Receiver<rboy::input::Event>,
    exit: Arc<AtomicBool>,
    games: Vec<GameEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    GameBoy,
    GameBoyColor,
}

#[derive(Debug, Clone)]
struct GameEntry {
    name: String,
    path: PathBuf,
    platform: Platform,
}

impl AppMenu {
    pub fn new(
        config: Rc<AppConfig>,
        framebuffer: Rc<Framebuffer>,
        exit: Arc<AtomicBool>,
        event_receiver: Receiver<rboy::input::Event>,
    ) -> anyhow::Result<Self> {
        // scan directory
        let mut games = vec![];
        if let Ok(entries) = std::fs::read_dir(&config.roms_directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    debug!("Skipping non-file entry: {:?}", path);
                    continue;
                }
                let Some(extension) = path.extension() else {
                    warn!("File without extension: {:?}", path);
                    continue;
                };
                // get name
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    warn!("Invalid file name: {:?}", path);
                    continue;
                };
                let extension = extension.to_string_lossy().to_lowercase();
                let platform = match extension.as_str() {
                    "gb" => Platform::GameBoy,
                    "gbc" => Platform::GameBoyColor,
                    _ => {
                        debug!("Unsupported file extension: {:?}", path);
                        continue;
                    }
                };
                // remove extension from name
                let name = name
                    .trim_end_matches(".gb")
                    .trim_end_matches(".gbc")
                    .to_string();
                info!(
                    "Found game: {name} for {platform:?} at {path}",
                    path = path.display()
                );
                games.push(GameEntry {
                    name: name.to_string(),
                    path,
                    platform,
                });
            }
        }

        Ok(Self {
            config,
            event_receiver,
            exit,
            framebuffer,
            games,
        })
    }

    pub fn run(self) -> anyhow::Result<AppState> {
        self.splash();

        let mut redraw = true;
        let mut selected = 0;

        loop {
            if self.exit.load(Ordering::Relaxed) {
                return Ok(AppState::Exit);
            }

            if redraw {
                self.redraw(selected);
                redraw = false;
            }

            // read input
            let (event, key) = match self.event_receiver.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                Err(TryRecvError::Disconnected) => {
                    self.exit.store(true, Ordering::Relaxed);
                    error!("Main thread disconnected");
                    return Ok(AppState::Exit);
                }
            };

            match (event, key) {
                (KeyEvent::Down, KeypadKey::Start) => {
                    let Some(path) = self.games.get(selected).map(|g| g.path.clone()) else {
                        error!("No such game at {selected}");
                        continue;
                    };
                    return Ok(AppState::Emulator {
                        rom_file: path,
                        config: self.config,
                    });
                }
                (KeyEvent::Down, KeypadKey::Up) => {
                    selected = selected.saturating_sub(1);
                    redraw = true;
                }
                (KeyEvent::Down, KeypadKey::Down) => {
                    if selected + 1 < self.games.len() {
                        selected = selected.saturating_add(1);
                        redraw = true;
                    }
                }
                _ => continue,
            }
        }
    }

    /// show splash and play bling
    fn splash(&self) {
        info!("Showing splash screen");
        self.framebuffer.fill(
            GAMEBOY_SPLASH_COLOR_RED,
            GAMEBOY_SPLASH_COLOR_GREEN,
            GAMEBOY_SPLASH_COLOR_BLUE,
        );

        // write at middle
        let mut y = self.framebuffer.height() - 4;
        let x = self
            .framebuffer
            .width()
            .saturating_sub(SPLASH_TEXT.len() / 2);

        self.draw_text(SPLASH_TEXT, x, &mut y, false, COLOR_BLACK);

        if let Err(err) = self.play_boot_sound() {
            error!("Failed to play boot sound: {err}");
        }

        // wait for 10 seconds
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            if self.exit.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Play boot sound
    fn play_boot_sound(&self) -> anyhow::Result<()> {
        let p = Path::new(GB_BOOT_FILEPATH);
        self.create_boot_sound(p)?;

        // play it
        let status = std::process::Command::new("aplay")
            .arg(p.display().to_string())
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to play boot sound"))
        }
    }

    /// Create boot sound file
    fn create_boot_sound(&self, p: &Path) -> anyhow::Result<()> {
        if p.exists() {
            return Ok(());
        }

        let mut f = File::create(GB_BOOT_FILEPATH)?;
        f.write_all(GB_BOOT_SOUND)?;

        Ok(())
    }

    fn redraw(&self, selected: usize) {
        debug!("Redraw menu");
        // zero
        self.framebuffer.zero();

        let max_visible = (self.framebuffer.height() / 16).saturating_sub(2); // title + subtitle (2)
        let skip = usize::clamp(
            selected.saturating_sub(max_visible / 2),
            0,
            usize::max(0, self.games.len().saturating_sub(max_visible)),
        );
        debug!("Skipping {skip} (max visible: {max_visible}) games");

        let mut y = PADDING_Y;

        // write title first
        self.draw_text(
            &format!(
                "{crate_name} {crate_version}",
                crate_name = env!("CARGO_PKG_NAME"),
                crate_version = env!("CARGO_PKG_VERSION")
            ),
            PADDING_X,
            &mut y,
            false,
            COLOR_WHITE,
        );
        self.draw_text(SUBTITLE, PADDING_X, &mut y, false, COLOR_WHITE);

        // write message if there are no games
        if self.games.is_empty() {
            self.draw_text(NO_GAMES, PADDING_X, &mut y, false, COLOR_WHITE);
            return;
        }

        for (i, game) in self.games.iter().skip(skip).take(max_visible).enumerate() {
            let x = PADDING_X; // padding
            let is_selected = skip + i == selected;
            let line = format!(
                "{} {} - {}",
                if is_selected { ">" } else { " " },
                game.name,
                match game.platform {
                    Platform::GameBoy => "GameBoy",
                    Platform::GameBoyColor => "GameBoyColor",
                }
            );
            self.draw_text(&line, x, &mut y, is_selected, COLOR_WHITE);
        }
    }

    /// Draw text
    fn draw_text(&self, text: &str, mut x: usize, y: &mut usize, invert: bool, color: u16) {
        debug!("Drawing text '{text}' at ({x}, {y}); invert: {invert}");
        for glyph in text.chars() {
            self.draw_char(x, *y, glyph, invert, color);
            x += SPACE_SIZE;
        }

        *y += LINE_H;
    }

    /// draw a character in the framebuffer
    fn draw_char(&self, x: usize, y: usize, c: char, invert: bool, color: u16) {
        let glyph = BASIC_FONTS.get(c).unwrap_or([0u8; 8]);
        debug!("Glyph for {c} ({x}, {y}): {glyph:?}");

        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                let mask = bits & (1 << col);
                if (!invert && mask != 0) || (invert && mask == 0) {
                    self.framebuffer.put_pixel(x + col, y + row, color);
                }
            }
        }
    }
}
