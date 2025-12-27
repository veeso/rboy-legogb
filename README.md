# RBoy-legogb

A Fork of [Rboy](https://github.com/mvdnes/rboy) to run on a Raspberry Pi with GPIO buttons as keys and framebuffer as video output.

## QuickStart

Then you can explore the ability of the emulator by `rboy-legogb --help`. Which outputs:

```txt
A Gameboy Colour emulator written in Rust

Usage: rboy [OPTIONS] <rom_filename>

Arguments:
  <filename>  Sets the ROM file to load

Options:
  --config <config>                   Sets the configuration file to use [default: rboy_config.toml]
  --framebuffer-path <framebuffer-path>
                                     Sets the framebuffer device path [default: /dev/fb0]
  --width <width>                     Sets the framebuffer width [default: 800]
  --height <height>                   Sets the framebuffer height [default: 600]
  --bytes-per-pixel <bytes-per-pixel>
                                     Sets the framebuffer bytes per pixel [default: 2]
  --stride-pixels <stride-pixels>     Sets the framebuffer stride in pixels [default: 800]
  --scale <scale>                     Sets the framebuffer scale [default: 1]
```

Now you can look below for the Keybindings section below.

## Configuration

Create a toml configuration file with the pinout configuration for GPIO buttons,

```toml
# roms directory
roms_directory = "/home/pi/roms"
# default debounce for all buttons (in milliseconds)
default_debounce_ms = 50
# default active low for all buttons
default_active_low = true
# polling interval for reading buttons (in milliseconds)
poll_interval_ms = 10

# D-Pad

[[key]]
# gpio pin number
gpio = 5
# associated keycode (UP, DOWN, LEFT, RIGHT, A, B, START, SELECT)
keycode = "UP"
# whether the key should auto-repeat when held down
repeat = true
# delay before starting to repeat (in milliseconds)
repeat_delay_ms = 300
# repeat rate (in milliseconds)
repeat_rate_ms = 80

[[key]]
gpio = 6
keycode = "DOWN"
repeat = true
repeat_delay_ms = 300
repeat_rate_ms = 80

[[key]]
gpio = 13
keycode = "LEFT"
repeat = true
repeat_delay_ms = 300
repeat_rate_ms = 80

[[key]]
gpio = 16
keycode = "RIGHT"
repeat = true
repeat_delay_ms = 300
repeat_rate_ms = 80

[[key]]
gpio = 17
keycode = "A"
repeat = false

[[key]]
gpio = 22
keycode = "B"
repeat = false

[[key]]
gpio = 23
keycode = "START"
repeat = false

[[key]]
gpio = 24
keycode = "SELECT"
repeat = false

[[powerswitch]]
gpio = 26
```
