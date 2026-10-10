use std::{
    ffi::CString, fmt, marker::PhantomData, os::unix::ffi::OsStrExt, path::Path, ptr::NonNull,
    rc::Rc,
};
/// Metadata for the first local video track, after its preferred orientation.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoInfo {
    pub duration_seconds: f64,
    pub width: u32,
    pub height: u32,
}
/// Rejected input, file I/O, unsupported/faulted native decode, or capacity limit.
#[derive(Debug)]
pub enum VideoError {
    InvalidInput,
    Io(std::io::Error),
    Decode,
    Capacity,
}
impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => f.write_str("invalid video input"),
            Self::Io(e) => e.fmt(f),
            Self::Decode => f.write_str("native video decoding failed"),
            Self::Capacity => f.write_str("video capacity exceeded"),
        }
    }
}
impl std::error::Error for VideoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
/// Owned top-to-bottom RGBA8 sRGB pixels with premultiplied alpha.
#[derive(Debug)]
pub struct VideoFrame {
    pub info: VideoInfo,
    pub actual_seconds: f64,
    pub pixels: Vec<u8>,
}
unsafe extern "C" {
    fn ravet_video_open(
        path: *const std::ffi::c_char,
        info: *mut VideoInfo,
        error: *mut u32,
    ) -> *mut std::ffi::c_void;
    fn ravet_video_frame(
        handle: *mut std::ffi::c_void,
        seconds: f64,
        output: *mut u8,
        capacity: usize,
        info: *mut VideoInfo,
        actual: *mut f64,
    ) -> u32;
    fn ravet_video_close(handle: *mut std::ffi::c_void);
}
fn error(code: u32) -> VideoError {
    match code {
        1 => VideoError::InvalidInput,
        4 => VideoError::Capacity,
        _ => VideoError::Decode,
    }
}
/// Thread-confined local first-track video reader. Create, extract and drop on
/// one UI/worker thread; every method may allocate or perform blocking native I/O.
/// Native backend transient work/memory are not bounded by returned-frame limits.
/// No audio callback, automatic playback, audio-track decoding, network or DRM API.
///
/// Native ownership cannot transfer to another thread:
/// ```compile_fail
/// use ravet_media::VideoReader;
/// fn requires_send<T: Send>() {}
/// requires_send::<VideoReader>();
/// ```
pub struct VideoReader {
    handle: NonNull<std::ffi::c_void>,
    info: VideoInfo,
    _thread: PhantomData<Rc<()>>,
}
impl VideoReader {
    /// Open an explicit regular file <=2 GiB, UTF8 path <=4096 bytes, no NUL.
    /// The first video track must have finite (0,24h] duration and oriented
    /// dimensions 1..4096. Codec/container support follows the local OS decoder;
    /// opening does not establish that every frame will decode successfully.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VideoError> {
        let path = path.as_ref();
        let bytes = path.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.len() > 4096 || std::str::from_utf8(bytes).is_err() {
            return Err(VideoError::InvalidInput);
        }
        let name = CString::new(bytes).map_err(|_| VideoError::InvalidInput)?;
        let metadata = path.metadata().map_err(VideoError::Io)?;
        if !metadata.is_file() {
            return Err(VideoError::InvalidInput);
        }
        if metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 * 1024 {
            return Err(VideoError::Capacity);
        }
        let mut info = VideoInfo {
            duration_seconds: 0.,
            width: 0,
            height: 0,
        };
        let mut status = 0;
        // SAFETY: native code copies the live terminated name synchronously and
        // writes only these disjoint ABI records, returning an exclusively owned handle.
        let handle = unsafe { ravet_video_open(name.as_ptr(), &mut info, &mut status) };
        let handle = NonNull::new(handle).ok_or_else(|| error(status))?;
        Ok(Self {
            handle,
            info,
            _thread: PhantomData,
        })
    }
    pub fn info(&self) -> VideoInfo {
        self.info
    }
    /// Request an exact decode timestamp in [0,duration). OS frame selection
    /// reports its actual timestamp; requests between source frames may reject.
    /// Returns <=64 MiB of owned RGBA pixels, with no partial frame on failure.
    /// Out-of-order requests perform independent extraction; no logical playback
    /// cursor/clock, cache, fallback or interpolation is inferred.
    pub fn frame_at(&mut self, seconds: f64) -> Result<VideoFrame, VideoError> {
        if !seconds.is_finite() || seconds < 0. || seconds >= self.info.duration_seconds {
            return Err(VideoError::InvalidInput);
        }
        let mut pixels = vec![0; self.info.width as usize * self.info.height as usize * 4];
        let mut info = self.info;
        let mut actual = 0.;
        // SAFETY: the live exclusive handle and complete bounded destination are
        // valid through synchronous extraction; records are disjoint and initialized.
        let status = unsafe {
            ravet_video_frame(
                self.handle.as_ptr(),
                seconds,
                pixels.as_mut_ptr(),
                pixels.len(),
                &mut info,
                &mut actual,
            )
        };
        if status != 0 {
            return Err(error(status));
        }
        pixels.truncate(info.width as usize * info.height as usize * 4);
        Ok(VideoFrame {
            info,
            actual_seconds: actual,
            pixels,
        })
    }
}
impl Drop for VideoReader {
    fn drop(&mut self) {
        // SAFETY: one exclusive live native handle, destroyed once on its owning thread.
        unsafe { ravet_video_close(self.handle.as_ptr()) }
    }
}
