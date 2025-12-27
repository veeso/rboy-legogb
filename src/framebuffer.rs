use std::os::fd::AsRawFd;
use std::path::PathBuf;

pub struct FramebufferConfig {
    pub path: PathBuf,
    pub width: usize,
    pub height: usize,
    pub bytes_per_pixel: usize,
    pub stride_pixels: usize,
    pub scale: usize,
}

/// Represents a memory-mapped framebuffer.
pub struct Framebuffer {
    height: usize,
    ptr: *mut u16,
    /// The number of pixels in a single row of the framebuffer.
    stride: usize,
    scale: usize,
}

impl Framebuffer {
    /// Creates a new [`Framebuffer`] mapped to the given path with the specified width and height.
    pub fn new(config: FramebufferConfig) -> anyhow::Result<Framebuffer> {
        // open framebuffer
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(config.path)?;

        let fd = file.as_raw_fd();

        let size = config.stride_pixels * config.height * config.bytes_per_pixel;

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        } as *mut u16;

        if ptr == libc::MAP_FAILED as *mut u16 {
            return Err(anyhow::anyhow!("Failed to mmap framebuffer"));
        }
        Ok(Framebuffer {
            scale: config.scale,
            height: config.height,
            ptr,
            stride: config.stride_pixels,
        })
    }

    pub fn write(&self, buf: &[u8]) {
        let scaled_h = crate::SCREEN_H * self.scale;
        let y_offset: isize = (self.height as isize - scaled_h as isize) / 2;

        debug!(
            "Writing framebuffer: y_offset = {}, scaled_h = {}",
            y_offset, scaled_h
        );

        for sy in 0..crate::SCREEN_H {
            let dy_base = (sy * self.scale) as isize + y_offset;

            // Out of bounds vertical
            if dy_base + (self.scale as isize) <= 0 || dy_base >= self.height as isize {
                continue;
            }

            for sx in 0..crate::SCREEN_W {
                let i = (sy * crate::SCREEN_W + sx) * 3;

                let r = buf[i];
                let g = buf[i + 1];
                let b = buf[i + 2];

                let rgb565: u16 =
                    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);

                let dx_base = sx * self.scale;

                // Draw a scale×scale block
                for py in 0..self.scale {
                    let dy = dy_base + py as isize;
                    if dy < 0 || dy >= self.height as isize {
                        continue;
                    }

                    unsafe {
                        let row = self.ptr.add(dy as usize * self.stride);

                        for px in 0..self.scale {
                            *row.add(dx_base + px) = rgb565;
                        }
                    }
                }
            }
        }
    }

    /// Fills the entire framebuffer with zeros.
    pub fn zero(&self) {
        let pixels = self.stride * self.height;
        unsafe {
            std::ptr::write_bytes(self.ptr, 0, pixels);
        }
    }
}
