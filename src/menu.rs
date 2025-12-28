use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

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
const TITLE: &str = "RBoy-lego - Press start to play a game";
const NO_GAMES: &str = "You have no games in your ROMs directory";

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
        let mut redraw = true;
        let mut selected = 0;

        loop {
            if self.exit.load(Ordering::Relaxed) {
                return Ok(AppState::Exit);
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
                (KeyEvent::Down, KeypadKey::Down) => {
                    selected = selected.saturating_sub(1);
                    redraw = true;
                }
                (KeyEvent::Down, KeypadKey::Up) => {
                    if selected + 1 < self.games.len() {
                        selected = selected.saturating_add(1);
                        redraw = true;
                    }
                }
                _ => continue,
            }

            if redraw {
                self.redraw(selected);
                redraw = false;
            }
        }
    }

    fn redraw(&self, selected: usize) {
        // zero
        self.framebuffer.zero();

        let max_visible = self.framebuffer.height() / 16;
        let top = max_visible.saturating_sub(selected);

        let mut y = PADDING_Y;

        // write title first
        self.draw_text(TITLE, PADDING_X, &mut y, false);

        // write message if there are no games
        if self.games.is_empty() {
            self.draw_text(NO_GAMES, PADDING_X, &mut y, false);
            return;
        }

        for (i, game) in self.games.iter().skip(top).take(max_visible).enumerate() {
            let x = PADDING_X; // padding
            let is_selected = top + i == selected;
            let line = format!(
                "{} {} - {}",
                if is_selected { "> " } else { "  " },
                game.name,
                match game.platform {
                    Platform::GameBoy => "GameBoy",
                    Platform::GameBoyColor => "GameBoyColor",
                }
            );
            self.draw_text(&line, x, &mut y, is_selected);
        }
    }

    /// Draw text
    fn draw_text(&self, text: &str, mut x: usize, y: &mut usize, invert: bool) {
        for glyph in text.chars() {
            self.draw_char(x, *y, glyph, invert);
            x += SPACE_SIZE;
        }

        *y += LINE_H;
    }

    /// draw a character in the framebuffer
    fn draw_char(&self, x: usize, y: usize, c: char, invert: bool) {
        let glyph = BASIC_FONTS.get(c).unwrap_or([0u8; 8]);

        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                let mask = bits & (1 << col);
                if (!invert && mask != 0) || (invert && mask == 0) {
                    self.framebuffer.put_pixel(x + col, y + row, 0xffff);
                }
            }
        }
    }
}
