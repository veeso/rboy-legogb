#![crate_name = "rboy"]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use rboy::device::Device;
use rboy::framebuffer::{Framebuffer, FramebufferConfig};
use rboy::input::gpio::RaspberryGpio;
use rboy::input::pinout::PinoutConfig;
use rboy::input::{InputListener, InputListenerConfig, KeyConfig, KeyEvent, PowerSwitch};
use std::io::{self, Read, Write};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

const EXITCODE_SUCCESS: i32 = 0;
const EXITCODE_CPU_LOAD_FAILS: i32 = 2;

enum GBEvent {
    KeyUp(rboy::KeypadKey),
    KeyDown(rboy::KeypadKey),
}

fn main() {
    let exit_status = real_main();
    if exit_status != EXITCODE_SUCCESS {
        std::process::exit(exit_status);
    }
}

fn real_main() -> i32 {
    let matches = clap::Command::new("rboy")
        .version("0.1")
        .author("Mathijs van de Nes")
        .about("A Gameboy Colour emulator written in Rust")
        .arg(
            clap::Arg::new("filename")
                .help("Sets the ROM file to load")
                .required(true),
        )
        .arg(
            clap::Arg::new("serial")
                .help("Prints the data from the serial port to stdout")
                .short('s')
                .long("serial")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("printer")
                .help("Emulates a gameboy printer")
                .short('p')
                .long("printer")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("classic")
                .help("Forces the emulator to run in classic Gameboy mode")
                .short('c')
                .long("classic")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("scale")
                .help("Sets the scale of the emulator window")
                .long("scale")
                .default_value("2"),
        )
        .arg(
            clap::Arg::new("width")
                .help("Sets the width of the emulator")
                .long("width"),
        )
        .arg(
            clap::Arg::new("height")
                .help("Sets the height of the emulator")
                .long("height"),
        )
        .arg(
            clap::Arg::new("stride-pixels")
                .help("Sets the stride (in pixels) of the framebuffer")
                .long("stride-pixels"),
        )
        .arg(
            clap::Arg::new("bytes-per-pixel")
                .help("Sets the bytes per pixel of the framebuffer")
                .long("bytes-per-pixel"),
        )
        .arg(
            clap::Arg::new("audio")
                .help("Enables audio")
                .short('a')
                .long("audio")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("skip-checksum")
                .help("Skips verification of the cartridge checksum")
                .long("skip-checksum")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("test-mode")
                .help("Starts the emulator in a special test mode")
                .long("test-mode")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            clap::Arg::new("state-path")
                .help("Starts the emulator from a saved state file at the specified path")
                .long("load-state"),
        )
        .arg(
            clap::Arg::new("framebuffer")
                .help("Specify the path to the framebuffer output file")
                .long("framebuffer")
                .default_value("/dev/fb0"),
        )
        .arg(
            clap::Arg::new("pinout")
                .help("Specify the path to the GPIO pinout configuration file")
                .long("pinout")
                .default_value("pinout.toml"),
        )
        .get_matches();

    let test_mode = matches.get_one::<bool>("test-mode").copied().unwrap();
    let opt_reload: Option<String> = matches
        .get_one::<String>("state-path")
        .map(|s| s.to_string());
    let opt_serial = matches.get_one::<bool>("serial").copied().unwrap();
    let opt_printer = matches.get_one::<bool>("printer").copied().unwrap();
    let opt_classic = matches.get_one::<bool>("classic").copied().unwrap();
    let opt_audio = matches.get_one::<bool>("audio").copied().unwrap();
    let opt_skip_checksum = matches.get_one::<bool>("skip-checksum").copied().unwrap();
    let filename = matches.get_one::<String>("filename").unwrap();

    if test_mode {
        return run_test_mode(filename, opt_classic, opt_skip_checksum);
    }

    let mut is_new_start = true;
    let cpu = opt_reload
        .as_ref()
        .filter(|path| std::path::Path::new(path).exists())
        .and_then(|path| {
            is_new_start = false;
            Device::load_state(path)
        })
        .or_else(|| construct_cpu(filename, opt_classic, opt_skip_checksum, opt_reload.clone()));

    let Some(mut cpu) = cpu else {
        return EXITCODE_CPU_LOAD_FAILS;
    };

    if opt_printer {
        cpu.attach_printer();
    } else {
        cpu.set_stdout(opt_serial);
    }

    let mut cpal_audio_stream = None;
    if opt_audio {
        let player = CpalPlayer::get();
        match player {
            Some((v, s)) => {
                cpu.enable_audio(Box::new(v) as Box<dyn rboy::AudioPlayer>, !is_new_start);
                cpal_audio_stream = Some(s);
            }
            None => {
                warn("Could not open audio device");
                return EXITCODE_CPU_LOAD_FAILS;
            }
        }
    }

    let width = matches
        .get_one::<String>("width")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(160);
    let height = matches
        .get_one::<String>("height")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(144);
    let stride_pixels = matches
        .get_one::<String>("stride-pixels")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(160);
    let bytes_per_pixel = matches
        .get_one::<String>("bytes-per-pixel")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2);
    let scale = matches
        .get_one::<String>("scale")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2);

    let framebuffer_path = std::path::Path::new(
        matches
            .get_one::<String>("framebuffer")
            .expect("Framebuffer path missing"),
    );

    let fb_config = FramebufferConfig {
        path: framebuffer_path.to_path_buf(),
        width,
        height,
        scale,
        stride_pixels,
        bytes_per_pixel,
    };
    let mut framebuffer = Framebuffer::new(fb_config).expect("Could not open framebuffer");

    let (gb_event_sender, gb_event_receiver) = mpsc::channel();
    let (video_sender, video_receiver) = mpsc::sync_channel(1);

    let cpu_thread = thread::spawn(move || run_cpu(cpu, video_sender, gb_event_receiver));

    let pinout_config_path = matches
        .get_one::<String>("pinout")
        .expect("Pinout path missing");
    let pinout_config_path = std::path::Path::new(pinout_config_path);
    let pinout_config = match PinoutConfig::load_from_file(pinout_config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn(&format!("Could not load pinout configuration: {}", e));
            return EXITCODE_CPU_LOAD_FAILS;
        }
    };

    // run input listener
    let exit_flag = Arc::new(AtomicBool::new(false));
    let (keyboard_event_sender, keyboard_event_receiver) = mpsc::channel();
    let input_listener_thread =
        run_input_listener(pinout_config, exit_flag.clone(), keyboard_event_sender);

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
                if let Err(err) = framebuffer.write(&data) {
                    warn(&format!("Could not write to framebuffer: {err}"));
                    break;
                }
            }
            Err(TryRecvError::Empty) => {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(TryRecvError::Disconnected) => break, // Remote end has hung-up
        }
    }

    // join
    exit_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = input_listener_thread.join();

    drop(cpal_audio_stream);
    drop(video_receiver); // Stop CPU thread by disconnecting
    let _ = cpu_thread.join();

    EXITCODE_SUCCESS
}

fn warn(message: &str) {
    eprintln!("{}", message);
}

fn construct_cpu(
    filename: &str,
    classic_mode: bool,
    skip_checksum: bool,
    reload_mode: Option<String>,
) -> Option<Box<Device>> {
    let opt_c = match classic_mode {
        true => Device::new(filename, skip_checksum, reload_mode),
        false => Device::new_cgb(filename, skip_checksum, reload_mode),
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
    thread::spawn(move || loop {
        thread::sleep(std::time::Duration::from_millis(ms));
        if tx.send(()).is_err() {
            break;
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

struct NullAudioPlayer {}

impl rboy::AudioPlayer for NullAudioPlayer {
    fn play(&mut self, _buf_left: &[f32], _buf_right: &[f32]) {
        // Do nothing
    }

    fn samples_rate(&self) -> u32 {
        44100
    }

    fn underflowed(&self) -> bool {
        false
    }
}

fn run_test_mode(filename: &str, classic_mode: bool, skip_checksum: bool) -> i32 {
    let opt_cpu = match classic_mode {
        true => Device::new(filename, skip_checksum, None),
        false => Device::new_cgb(filename, skip_checksum, None),
    };
    let mut cpu = match opt_cpu {
        Err(errmsg) => {
            warn(errmsg);
            return EXITCODE_CPU_LOAD_FAILS;
        }
        Ok(cpu) => cpu,
    };

    cpu.set_stdout(true);
    cpu.enable_audio(Box::new(NullAudioPlayer {}), false);

    // from masonforest, https://stackoverflow.com/a/55201400 (CC BY-SA 4.0)
    let stdin_channel = spawn_stdin_channel();
    loop {
        match stdin_channel.try_recv() {
            Ok(stdin_byte) => match stdin_byte {
                b'q' => break,
                b's' => {
                    let data = cpu.get_gpu_data().to_vec();
                    print_screenshot(data);
                }
                v => {
                    eprintln!("MSG:Unknown stdinvalue {}", v);
                }
            },
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        for _ in 0..1000 {
            cpu.do_cycle();
        }
    }
    EXITCODE_SUCCESS
}

fn spawn_stdin_channel() -> Receiver<u8> {
    let (tx, rx) = mpsc::channel::<u8>();
    thread::spawn(move || loop {
        let mut buffer = [0];
        match io::stdin().read(&mut buffer) {
            Ok(1) => tx.send(buffer[0]).unwrap(),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            _ => break,
        };
    });
    rx
}

fn print_screenshot(data: Vec<u8>) {
    eprint!("SCREENSHOT:");
    for b in data {
        eprint!("{:02x}", b);
    }
    eprintln!();
}

fn run_input_listener(
    config: PinoutConfig,
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
