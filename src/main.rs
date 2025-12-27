mod app_config;
mod args;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use rboy::device::Device;
use rboy::framebuffer::{Framebuffer, FramebufferConfig};
use rboy::input::gpio::RaspberryGpio;
use rboy::input::{InputListener, InputListenerConfig, KeyConfig, KeyEvent, PowerSwitch};

use self::app_config::AppConfig;

enum GBEvent {
    KeyUp(rboy::KeypadKey),
    KeyDown(rboy::KeypadKey),
}

/// The Application state.
#[derive(Debug, Clone)]
enum AppState {
    Emulator {
        config: Rc<AppConfig>,
        rom_file: PathBuf,
    },
    Menu {
        config: Rc<AppConfig>,
    },
    Exit,
}

fn main() -> anyhow::Result<()> {
    let args: args::Args = argh::from_env();

    // read config
    let config = Rc::new(AppConfig::load_from_file(&args.config)?);

    // open framebuffer
    let framebuffer = Rc::new(Framebuffer::new(FramebufferConfig {
        path: args.framebuffer_path,
        width: args.width,
        height: args.height,
        bytes_per_pixel: args.bytes_per_pixel,
        stride_pixels: args.stride_pixels,
        scale: args.scale,
    })?);

    // init state
    let mut app_state = match &args.rom_path {
        Some(rom_path) => AppState::Emulator {
            config: config.clone(),
            rom_file: rom_path.clone(),
        },
        None => AppState::Menu {
            config: config.clone(),
        },
    };

    // setup control c handler
    let exit = Arc::new(AtomicBool::new(false));
    {
        let exit = exit.clone();
        ctrlc::set_handler(move || {
            exit.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .expect("Error setting Ctrl-C handler");
    }

    // loop through state machine

    loop {
        app_state = match app_state {
            AppState::Emulator { config, rom_file } => {
                run_emulator(&rom_file, config, framebuffer.clone())?
            }
            AppState::Menu { config } => {
                todo!();
            }
            AppState::Exit => break,
        };
    }

    Ok(())
}

fn run_emulator(
    rom_file: &Path,
    config: Rc<AppConfig>,
    framebuffer: Rc<Framebuffer>,
) -> anyhow::Result<AppState> {
    // zero framebuffer
    framebuffer.zero();

    let cpu = construct_cpu(rom_file, false, false, None);

    let Some(mut cpu) = cpu else {
        return Err(anyhow::anyhow!("Could not construct CPU"));
    };

    let cpal_audio_stream;

    let player = CpalPlayer::get();
    match player {
        Some((v, s)) => {
            cpu.enable_audio(Box::new(v) as Box<dyn rboy::AudioPlayer>, false);
            cpal_audio_stream = Some(s);
        }
        None => {
            anyhow::bail!("Could not initialize audio device");
        }
    }
    let (gb_event_sender, gb_event_receiver) = mpsc::channel();
    let (video_sender, video_receiver) = mpsc::sync_channel(1);

    let cpu_thread = thread::spawn(move || run_cpu(cpu, video_sender, gb_event_receiver));

    // run input listener
    let exit_flag = Arc::new(AtomicBool::new(false));
    let (keyboard_event_sender, keyboard_event_receiver) = mpsc::channel();
    let input_listener_thread =
        run_input_listener(&config, exit_flag.clone(), keyboard_event_sender);

    loop {
        if exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        if let Ok((event, key)) = keyboard_event_receiver.try_recv() {
            match event {
                KeyEvent::Down => {
                    let _ = gb_event_sender.send(GBEvent::KeyDown(key));
                }
                KeyEvent::Up => {
                    let _ = gb_event_sender.send(GBEvent::KeyUp(key));
                }
            }
        }

        match video_receiver.try_recv() {
            Ok(data) => {
                framebuffer.write(&data);
            }
            Err(TryRecvError::Empty) => {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(TryRecvError::Disconnected) => break, // Remote end has hung-up
        }
    }

    let _ = input_listener_thread.join();

    drop(cpal_audio_stream);
    drop(video_receiver); // Stop CPU thread by disconnecting
    let _ = cpu_thread.join();

    // zero framebuffer
    framebuffer.zero();

    if exit_flag.load(std::sync::atomic::Ordering::SeqCst) {
        Ok(AppState::Exit)
    } else {
        Ok(AppState::Menu { config })
    }
}

fn warn(message: &str) {
    eprintln!("{}", message);
}

fn construct_cpu(
    rom_file: &Path,
    classic_mode: bool,
    skip_checksum: bool,
    reload_mode: Option<String>,
) -> Option<Box<Device>> {
    let opt_c = match classic_mode {
        true => Device::new(rom_file, skip_checksum, reload_mode),
        false => Device::new_cgb(rom_file, skip_checksum, reload_mode),
    };
    let c = match opt_c {
        Ok(cpu) => cpu,
        Err(message) => {
            warn(message);
            return None;
        }
    };

    Some(Box::new(c))
}

fn run_cpu(mut cpu: Box<Device>, sender: SyncSender<Vec<u8>>, receiver: Receiver<GBEvent>) {
    let periodic = timer_periodic(16);

    let waitticks = (4194304f64 / 1000.0 * 16.0).round() as u32;
    let mut ticks = 0;

    'outer: loop {
        while ticks < waitticks {
            ticks += cpu.do_cycle();
            if cpu.check_and_reset_gpu_updated() {
                let data = cpu.get_gpu_data().to_vec();
                if let Err(TrySendError::Disconnected(..)) = sender.try_send(data) {
                    break 'outer;
                }
            }
        }

        ticks -= waitticks;

        'recv: loop {
            match receiver.try_recv() {
                Ok(event) => match event {
                    GBEvent::KeyUp(key) => cpu.keyup(key),
                    GBEvent::KeyDown(key) => cpu.keydown(key),
                },
                Err(TryRecvError::Empty) => break 'recv,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }

        let _ = periodic.recv();
    }
}

fn timer_periodic(ms: u64) -> Receiver<()> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_millis(ms));
            if tx.send(()).is_err() {
                break;
            }
        }
    });
    rx
}

struct CpalPlayer {
    buffer: Arc<Mutex<Vec<(f32, f32)>>>,
    sample_rate: u32,
}

impl CpalPlayer {
    fn get() -> Option<(CpalPlayer, cpal::Stream)> {
        let device = match cpal::default_host().default_output_device() {
            Some(e) => e,
            None => return None,
        };

        // We want a config with:
        // chanels = 2
        // SampleFormat F32
        // Rate at around 44100

        let wanted_samplerate = cpal::SampleRate(44100);
        let supported_configs = match device.supported_output_configs() {
            Ok(e) => e,
            Err(_) => return None,
        };
        let mut supported_config = None;
        for f in supported_configs {
            if f.channels() == 2 && f.sample_format() == cpal::SampleFormat::F32 {
                if f.min_sample_rate() <= wanted_samplerate
                    && wanted_samplerate <= f.max_sample_rate()
                {
                    supported_config = Some(f.with_sample_rate(wanted_samplerate));
                } else {
                    supported_config = Some(f.with_max_sample_rate());
                }
                break;
            }
        }
        if supported_config.is_none() {
            return None;
        }

        let selected_config = supported_config.unwrap();

        let sample_format = selected_config.sample_format();
        let config: cpal::StreamConfig = selected_config.into();

        let err_fn = |err| eprintln!("An error occurred on the output audio stream: {}", err);

        let shared_buffer = Arc::new(Mutex::new(Vec::new()));
        let stream_buffer = shared_buffer.clone();

        let player = CpalPlayer {
            buffer: shared_buffer,
            sample_rate: config.sample_rate.0,
        };

        let stream = match sample_format {
            cpal::SampleFormat::I8 => device.build_output_stream(
                &config,
                move |data: &mut [i8], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_output_stream(
                &config,
                move |data: &mut [i32], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::I64 => device.build_output_stream(
                &config,
                move |data: &mut [i64], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U8 => device.build_output_stream(
                &config,
                move |data: &mut [u8], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_output_stream(
                &config,
                move |data: &mut [u16], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U32 => device.build_output_stream(
                &config,
                move |data: &mut [u32], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::U64 => device.build_output_stream(
                &config,
                move |data: &mut [u64], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            cpal::SampleFormat::F64 => device.build_output_stream(
                &config,
                move |data: &mut [f64], _callback_info: &cpal::OutputCallbackInfo| {
                    cpal_thread(data, &stream_buffer)
                },
                err_fn,
                None,
            ),
            sf => panic!("Unsupported sample format {}", sf),
        }
        .unwrap();

        stream.play().unwrap();

        Some((player, stream))
    }
}

fn cpal_thread<T: Sample + FromSample<f32>>(
    outbuffer: &mut [T],
    audio_buffer: &Arc<Mutex<Vec<(f32, f32)>>>,
) {
    let mut inbuffer = audio_buffer.lock().unwrap();
    let outlen = std::cmp::min(outbuffer.len() / 2, inbuffer.len());
    for (i, (in_l, in_r)) in inbuffer.drain(..outlen).enumerate() {
        outbuffer[i * 2] = T::from_sample(in_l);
        outbuffer[i * 2 + 1] = T::from_sample(in_r);
    }
}

impl rboy::AudioPlayer for CpalPlayer {
    fn play(&mut self, buf_left: &[f32], buf_right: &[f32]) {
        debug_assert!(buf_left.len() == buf_right.len());

        let mut buffer = self.buffer.lock().unwrap();

        for (l, r) in buf_left.iter().zip(buf_right) {
            if buffer.len() > self.sample_rate as usize {
                // Do not fill the buffer with more than 1 second of data
                // This speeds up the resync after the turning on and off the speed limiter
                return;
            }
            buffer.push((*l, *r));
        }
    }

    fn samples_rate(&self) -> u32 {
        self.sample_rate
    }

    fn underflowed(&self) -> bool {
        (*self.buffer.lock().unwrap()).len() == 0
    }
}

fn run_input_listener(
    config: &AppConfig,
    exit: Arc<AtomicBool>,
    event_sender: Sender<rboy::input::Event>,
) -> JoinHandle<()> {
    let poll_interval = config.poll_interval();
    let power_switches = config
        .power_switches
        .iter()
        .map(|ps| PowerSwitch {
            gpio: gpio(ps.gpio, ps.active_low.unwrap_or(config.default_active_low)),
        })
        .collect();
    let keys = config
        .keys
        .iter()
        .map(|kc| KeyConfig {
            gpio: gpio(kc.gpio, kc.active_low.unwrap_or(config.default_active_low)),
            keycode: kc.keycode.keycode(),
            debounce: kc.debounce().unwrap_or(config.default_debounce()),
            repeat: if kc.repeat {
                Some(rboy::input::RepeatConfig {
                    delay: kc
                        .repeat_delay()
                        .expect("Repeat delay must be set if repeat is true"),
                    rate: kc
                        .repeat_rate()
                        .expect("Repeat rate must be set if repeat is true"),
                })
            } else {
                None
            },
        })
        .collect();

    let config = InputListenerConfig {
        exit,
        power_switches,
        keys,
        poll_interval,
    };
    thread::spawn(move || InputListener::new(config, event_sender).run())
}

fn gpio(pin: u8, active_low: bool) -> RaspberryGpio {
    RaspberryGpio::try_new(pin, active_low).expect("Could not connect to GPIO")
}
