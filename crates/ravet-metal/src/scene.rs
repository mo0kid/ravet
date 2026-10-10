use super::{MetalBitmap, MetalError, MetalImage, MetalPaint, MetalRenderer, NativePaint};
use ravet_surface::{drawing::Color, layout::Rect};
use std::{ffi::c_void, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeImage {
    width: u32,
    height: u32,
    pixels: *const u8,
    bytes: usize,
}
unsafe extern "C" {
    fn ravet_metal_render_scene(
        handle: *mut c_void,
        paints: *const NativePaint,
        count: u32,
        images: *const NativeImage,
        image_count: u32,
        clear: *const f32,
        output: *mut u8,
        bytes: usize,
    ) -> u32;
}
fn bounds(r: Rect, width: u32, height: u32) -> Option<[f32; 4]> {
    let x = r.x().clamp(0., f64::from(width)) as f32;
    let y = r.y().clamp(0., f64::from(height)) as f32;
    let right = (r.x() + r.width()).clamp(0., f64::from(width)) as f32;
    let bottom = (r.y() + r.height()).clamp(0., f64::from(height)) as f32;
    (right > x && bottom > y).then_some([x, y, right, bottom])
}
impl MetalRenderer {
    /// Draw ordered rectangles and complete bitmap images into one cleared
    /// owned readback. Nearest sampling and source-over use numeric premultiplied
    /// RGBA8 with no gamma/color transform. Destination clipping proportionally
    /// adjusts source UV; f32 quantization can collapse UV to a constant sample.
    /// At most prepared raw commands,16 distinct referenced objects,64 MiB
    /// distinct uploads and67,108,864 conservative clear/covered-pixel work.
    /// Even fully clipped images consume referenced-input budgets. Complete
    /// preflight precedes native upload/submission; backend failure returns no
    /// partial public image. Source borrows last only through this synchronous
    /// UI/worker call; native uploads/driver work remain unbounded in time/memory.
    pub fn render_scene(
        &mut self,
        background: Color,
        paints: &[MetalPaint<'_>],
    ) -> Result<MetalImage, MetalError> {
        if paints.len() > self.maximum {
            return Err(MetalError::Capacity);
        }
        self.scene.clear();
        let mut sources: [Option<&MetalBitmap>; 16] = [None; 16];
        let mut images = [NativeImage {
            width: 0,
            height: 0,
            pixels: ptr::null(),
            bytes: 0,
        }; 16];
        let mut source_count = 0;
        let mut upload_bytes = 0usize;
        let mut work = u64::from(self.width) * u64::from(self.height);
        for paint in paints {
            let (destination, color, image_index) = match *paint {
                MetalPaint::Rectangle(p) => (p.rectangle, p.color, u32::MAX),
                MetalPaint::Image {
                    destination,
                    source,
                } => {
                    let found = sources[..source_count]
                        .iter()
                        .position(|p| ptr::eq(p.unwrap(), source));
                    let index = if let Some(index) = found {
                        index
                    } else {
                        if source_count == 16 || source.pixels().len() > 67_108_864 - upload_bytes {
                            return Err(MetalError::Capacity);
                        }
                        let index = source_count;
                        upload_bytes += source.pixels().len();
                        sources[index] = Some(source);
                        images[index] = NativeImage {
                            width: source.width(),
                            height: source.height(),
                            pixels: source.pixels().as_ptr(),
                            bytes: source.pixels().len(),
                        };
                        source_count += 1;
                        index
                    };
                    (destination, Color::rgb(255, 255, 255), index as u32)
                }
            };
            let Some(b) = bounds(destination, self.width, self.height) else {
                continue;
            };
            work += (f64::from(b[2]).ceil() - f64::from(b[0]).floor()) as u64
                * (f64::from(b[3]).ceil() - f64::from(b[1]).floor()) as u64;
            if work > 67_108_864 {
                return Err(MetalError::Capacity);
            }
            let uv = if image_index == u32::MAX {
                [0.; 4]
            } else {
                [
                    ((f64::from(b[0]) - destination.x()) / destination.width()).clamp(0., 1.)
                        as f32,
                    ((f64::from(b[1]) - destination.y()) / destination.height()).clamp(0., 1.)
                        as f32,
                    ((f64::from(b[2]) - destination.x()) / destination.width()).clamp(0., 1.)
                        as f32,
                    ((f64::from(b[3]) - destination.y()) / destination.height()).clamp(0., 1.)
                        as f32,
                ]
            };
            self.scene.push(NativePaint {
                bounds: b,
                uv,
                color: [
                    f32::from(color.red) / 255.,
                    f32::from(color.green) / 255.,
                    f32::from(color.blue) / 255.,
                    f32::from(color.alpha) / 255.,
                ],
                image_index,
                reserved: [0; 3],
            });
        }
        let clear = [
            f32::from(background.red) / 255.,
            f32::from(background.green) / 255.,
            f32::from(background.blue) / 255.,
            f32::from(background.alpha) / 255.,
        ];
        let mut pixels = vec![0; self.width as usize * self.height as usize * 4];
        // SAFETY: exclusively owned live context, complete validated draw metadata,
        // immutable exact-size source assets and disjoint full output remain live
        // for synchronous native upload/submission/wait/readback only.
        let status = unsafe {
            ravet_metal_render_scene(
                self.handle.as_ptr(),
                self.scene.as_ptr(),
                self.scene.len() as u32,
                images.as_ptr(),
                source_count as u32,
                clear.as_ptr(),
                pixels.as_mut_ptr(),
                pixels.len(),
            )
        };
        if status != 0 {
            return Err(MetalError::Backend);
        }
        Ok(MetalImage {
            width: self.width,
            height: self.height,
            pixels,
        })
    }
}
