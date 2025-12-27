pub mod config;
pub mod gpio;
pub mod pinout;
pub mod state;

pub use self::config::{InputListenerConfig, KeyConfig, PowerSwitch, RepeatConfig};
use self::gpio::{Gpio, GpioValue};
use self::state::KeyState;
use self::state::OutEvent;
use crate::KeypadKey;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KeyEvent {
    Up,
    Down,
}

pub type Event = (KeyEvent, KeypadKey);

/// Input listener.
///
/// The input listener monitors GPIOs and emits key events via the keyboard interface.
pub struct InputListener<GPIO>
where
    GPIO: Gpio,
{
    exit: Arc<AtomicBool>,
    event_sender: Sender<Event>,
    keys: Vec<KeyState<GPIO>>,
    power_switches: Vec<PowerSwitch<GPIO>>,
    poll_interval: Duration,
}

impl<G> InputListener<G>
where
    G: Gpio,
{
    /// Create a new input listener with the given configuration
    pub fn new(config: InputListenerConfig<G>, event_sender: Sender<Event>) -> Self {
        InputListener {
            exit: config.exit,
            event_sender,
            keys: config.keys.into_iter().map(KeyState::from).collect(),
            power_switches: config.power_switches,
            poll_interval: config.poll_interval,
        }
    }

    /// Run the input listener
    pub fn run(mut self) {
        while !self.exit.load(std::sync::atomic::Ordering::SeqCst) {
            for key in &mut self.keys {
                Self::handle_key_poll(key, &mut self.event_sender);
            }
            for switch in &mut self.power_switches {
                Self::handle_power_switch_poll(switch, &self.exit);
            }
            trace!("tick");
            std::thread::sleep(self.poll_interval);
        }
    }

    /// Handle polling of a single key
    fn handle_key_poll(key: &mut KeyState<G>, sender: &mut Sender<Event>) {
        // read value
        trace!("Polling key {:?}", key.keycode);
        let Ok(value) = key.gpio.read() else {
            error!("Failed to read GPIO for key {:?}", key.keycode);
            return;
        };
        trace!("Read GPIO value {:?} for key {:?}", value, key.keycode);
        // handle value
        let res = match key.handle_gpio_value(value) {
            OutEvent::None => Ok(()),
            OutEvent::Press => {
                info!("Key {:?} pressed", key.keycode);
                sender.send((KeyEvent::Down, key.keycode))
            }
            OutEvent::Release => {
                info!("Key {:?} released", key.keycode);
                sender.send((KeyEvent::Up, key.keycode))
            }
            OutEvent::Repeat => {
                info!("Key {:?} repeat", key.keycode);
                sender.send((KeyEvent::Down, key.keycode))
            }
        };
        if let Err(e) = res {
            error!("Failed to send key event for key {:?}: {}", key.keycode, e);
        }
    }

    /// Handle polling of a single power switch
    fn handle_power_switch_poll(switch: &mut PowerSwitch<G>, exit: &Arc<AtomicBool>) {
        let value = match switch.gpio.read() {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to read GPIO for power switch: {}", e);
                return;
            }
        };
        if value == GpioValue::Enabled {
            warn!("Power switch activated, shutting down system");
            #[cfg(target_os = "linux")]
            {
                use std::process::Command;
                if let Err(e) = Command::new("shutdown").arg("-h").arg("now").spawn() {
                    error!("Failed to execute shutdown command: {}", e);
                }
                exit.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}
