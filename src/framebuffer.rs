use std::io::Write;
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
}

impl Write for Framebuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let crop_top = (crate::SCREEN_H * self.scale - self.height) / self.scale;

        for sy in 0..crate::SCREEN_H {
            let dy0 = sy * self.scale;
            let dy1 = dy0 + 1;
            if dy1 < crop_top || dy0 >= crop_top + self.height {
                continue;
            }

            let dst_y0 = dy0.saturating_sub(crop_top);
            let dst_y1 = dy1.saturating_sub(crop_top);

            for sx in 0..crate::SCREEN_W {
                let i = (sy * crate::SCREEN_W + sx) * 3;

                let r = buf[i];
                let g = buf[i + 1];
                let b = buf[i + 2];

                // RGB888 to RGB565
                let rgb565: u16 =
                    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3);

                let dx0 = sx * self.scale;
                let dx1 = dx0 + 1;

                unsafe {
                    let row0 = self.ptr.add(dst_y0 * self.stride);
                    let row1 = self.ptr.add(dst_y1 * self.stride);

                    *row0.wrapping_add(dx0) = rgb565;
                    *row0.wrapping_add(dx1) = rgb565;
                    *row1.wrapping_add(dx0) = rgb565;
                    *row1.wrapping_add(dx1) = rgb565;
                }
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
