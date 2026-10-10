#[path = "bitmap.rs"]
mod bitmap;
#[path = "scene.rs"]
mod scene;
pub use bitmap::{MetalBitmap, MetalPaint};
use ravet_surface::{drawing::Color, layout::Rect};
use std::{ffi::c_void, fmt, marker::PhantomData, ptr::NonNull, rc::Rc};
/// Invalid dimensions/configuration, bounded work/capacity, or native GPU failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetalError {
    InvalidInput,
    Capacity,
    Backend,
}
impl fmt::Display for MetalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidInput => "invalid Metal drawing input",
            Self::Capacity => "Metal drawing capacity exceeded",
            Self::Backend => "native Metal drawing failed",
        })
    }
}
impl std::error::Error for MetalError {}
/// Ordered filled rectangle using portable geometry and straight RGBA components.
/// Rasterization clips and quantizes coordinates to f32, with no antialiasing,
/// gamma transform, rounded corners, path/image/text or implicit window.
#[derive(Clone, Copy, Debug)]
pub struct MetalRectangle {
    pub rectangle: Rect,
    pub color: Color,
}
/// Owned top-to-bottom packed RGBA8Unorm pixels, premultiplied alpha; components
/// retain their numeric convention with no implicit gamma/color-space transform.
#[derive(Debug)]
pub struct MetalImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
#[repr(C)]
struct NativeRectangle {
    x: f32,
    y: f32,
    right: f32,
    bottom: f32,
    red: f32,
    green: f32,
    blue: f32,
    alpha: f32,
}
#[repr(C)]
struct NativePaint {
    bounds: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    image_index: u32,
    reserved: [u32; 3],
}
unsafe extern "C" {
    fn ravet_metal_create(width: u32, height: u32, maximum: u32, error: *mut u32) -> *mut c_void;
    fn ravet_metal_render(
        handle: *mut c_void,
        rectangles: *const NativeRectangle,
        count: u32,
        background: *const f32,
        output: *mut u8,
        capacity: usize,
    ) -> u32;
    fn ravet_metal_close(handle: *mut c_void);
}
/// Thread-confined prepared Metal device/queue/pipeline/texture and rectangle buffer.
/// Creation, submission, waiting/readback and destruction may allocate/block and
/// belong to one UI/worker thread. No audio callback or deadline guarantee.
/// Requires the system-selected Metal GPU; no software fallback is inferred.
/// ```compile_fail
/// use ravet_metal::MetalRenderer;
/// fn needs_send<T:Send>(){}
/// needs_send::<MetalRenderer>();
/// ```
pub struct MetalRenderer {
    handle: NonNull<c_void>,
    width: u32,
    height: u32,
    maximum: usize,
    staging: Vec<NativeRectangle>,
    scene: Vec<NativePaint>,
    _thread: PhantomData<Rc<()>>,
}
impl MetalRenderer {
    /// Prepare 1..4096 dimensions and 0..4096 input rectangles. Output is <=64 MiB;
    /// per-render conservative clear/covered-pixel work is capped at 67,108,864.
    /// Native transient resources/driver work remain outside total-memory/time caps.
    pub fn new(width: u32, height: u32, maximum_rectangles: usize) -> Result<Self, MetalError> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(MetalError::InvalidInput);
        }
        if maximum_rectangles > 4096 {
            return Err(MetalError::Capacity);
        }
        let mut error = 0;
        // SAFETY: disjoint initialized status; native returns one exclusively owned handle.
        let handle =
            unsafe { ravet_metal_create(width, height, maximum_rectangles as u32, &mut error) };
        let handle = NonNull::new(handle).ok_or(MetalError::Backend)?;
        Ok(Self {
            handle,
            width,
            height,
            maximum: maximum_rectangles,
            staging: Vec::with_capacity(maximum_rectangles),
            scene: Vec::with_capacity(maximum_rectangles),
            _thread: PhantomData,
        })
    }
    /// Render a complete cleared image with ordered source-over rectangle blending.
    /// Input/work faults reject before GPU submission; native failures return no
    /// partial public image. Independent earlier returned images stay owned/stable.
    /// Fully clipped/empty/quantized-degenerate rectangles consume input capacity
    /// but produce no instance. Returned readback allocates; this is not realtime.
    pub fn render(
        &mut self,
        background: Color,
        rectangles: &[MetalRectangle],
    ) -> Result<MetalImage, MetalError> {
        if rectangles.len() > self.maximum {
            return Err(MetalError::Capacity);
        }
        self.staging.clear();
        let mut work = u64::from(self.width) * u64::from(self.height);
        for rectangle in rectangles {
            let r = rectangle.rectangle;
            let x = r.x().clamp(0., f64::from(self.width)) as f32;
            let y = r.y().clamp(0., f64::from(self.height)) as f32;
            let right = (r.x() + r.width()).clamp(0., f64::from(self.width)) as f32;
            let bottom = (r.y() + r.height()).clamp(0., f64::from(self.height)) as f32;
            if right <= x || bottom <= y {
                continue;
            }
            work += (f64::from(right).ceil() - f64::from(x).floor()) as u64
                * (f64::from(bottom).ceil() - f64::from(y).floor()) as u64;
            if work > 67_108_864 {
                return Err(MetalError::Capacity);
            }
            let c = rectangle.color;
            self.staging.push(NativeRectangle {
                x,
                y,
                right,
                bottom,
                red: f32::from(c.red) / 255.,
                green: f32::from(c.green) / 255.,
                blue: f32::from(c.blue) / 255.,
                alpha: f32::from(c.alpha) / 255.,
            });
        }
        let clear = [
            f32::from(background.red) / 255.,
            f32::from(background.green) / 255.,
            f32::from(background.blue) / 255.,
            f32::from(background.alpha) / 255.,
        ];
        let mut pixels = vec![0; self.width as usize * self.height as usize * 4];
        // SAFETY: live exclusive native context, complete initialized destination and
        // disjoint bounded staged rectangles/clear components valid through synchronous submission.
        let status = unsafe {
            ravet_metal_render(
                self.handle.as_ptr(),
                self.staging.as_ptr(),
                self.staging.len() as u32,
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
impl Drop for MetalRenderer {
    fn drop(&mut self) {
        // SAFETY: exclusive live context destroyed once on its owner thread.
        unsafe { ravet_metal_close(self.handle.as_ptr()) }
    }
}
