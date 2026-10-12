//! Minimal read-only CoreAudio/CF boundary. CPAL keeps AudioDeviceID private;
//! resolve its public UID rather than relying on private layout or stream ASBD.
#![allow(unsafe_code)]
use cpal::traits::DeviceTrait;
use std::ffi::c_void;

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}
impl PropertyAddress {
    fn global(selector: [u8; 4]) -> Self {
        Self {
            selector: u32::from_be_bytes(selector),
            scope: u32::from_be_bytes(*b"glob"),
            element: 0,
        }
    }
}
#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> *const c_void;
    fn CFRelease(object: *const c_void);
}
struct OwnedString(*const c_void);
impl Drop for OwnedString {
    fn drop(&mut self) {
        // SAFETY: constructed only from a non-null +1 CFString create result.
        unsafe { CFRelease(self.0) }
    }
}
fn check(status: i32, size: u32, expected: usize) -> Result<(), cpal::Error> {
    if status != 0 || size as usize != expected {
        return Err(cpal::Error::with_message(
            cpal::ErrorKind::BackendError,
            format!("CoreAudio nominal rate query: status {status}, data size {size}"),
        ));
    }
    Ok(())
}
/// Nominal rates are whole numbers in practice, but some drivers report them
/// with float noise (e.g. 44099.998); accept values within 0.01 Hz of an
/// integer and reject genuinely fractional rates.
fn integer_rate(rate: f64) -> Result<u32, cpal::Error> {
    if !rate.is_finite()
        || !(1.0..=f64::from(u32::MAX)).contains(&rate.round())
        || (rate - rate.round()).abs() > 0.01
    {
        return Err(cpal::Error::with_message(
            cpal::ErrorKind::BackendError,
            format!("CoreAudio reported an invalid/noninteger nominal sample rate: {rate}"),
        ));
    }
    Ok(rate.round() as u32)
}
pub(super) fn nominal_rate(device: &cpal::Device) -> Result<u32, cpal::Error> {
    let id = device.id()?;
    let bytes = id.id().as_bytes();
    let length = isize::try_from(bytes.len())
        .map_err(|_| cpal::Error::new(cpal::ErrorKind::InvalidInput))?;
    // SAFETY: borrowed UTF-8 bytes live through the synchronous CF copy. Null
    // allocator selects the default; 0x08000100 is kCFStringEncodingUTF8.
    let string =
        unsafe { CFStringCreateWithBytes(std::ptr::null(), bytes.as_ptr(), length, 0x08000100, 0) };
    if string.is_null() {
        return Err(cpal::Error::new(cpal::ErrorKind::ResourceExhausted));
    }
    let uid = OwnedString(string);
    let mut object = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: kAudioObjectSystemObject=1; TranslateUIDToDevice takes a pointer
    // to a live CFStringRef qualifier and writes one AudioObjectID. All pointers
    // have the required alignment and remain live for the synchronous call.
    let status = unsafe {
        AudioObjectGetPropertyData(
            1,
            &PropertyAddress::global(*b"uidd"),
            size_of::<*const c_void>() as u32,
            (&uid.0 as *const *const c_void).cast(),
            &mut size,
            (&mut object as *mut u32).cast(),
        )
    };
    check(status, size, size_of::<u32>())?;
    if object == 0 {
        return Err(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));
    }
    let mut rate = 0.0f64;
    size = size_of::<f64>() as u32;
    // SAFETY: NominalSampleRate is a Float64 global/main property. The resolved
    // object may disappear; CoreAudio then reports an error. Output is one live,
    // aligned f64 and its size/status are checked before accepting the value.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            &PropertyAddress::global(*b"nsrt"),
            0,
            std::ptr::null(),
            &mut size,
            (&mut rate as *mut f64).cast(),
        )
    };
    check(status, size, size_of::<f64>())?;
    integer_rate(rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nominal_property_values_and_receipts_are_checked_without_devices() {
        assert_eq!(integer_rate(96000.0).unwrap(), 96000);
        assert_eq!(integer_rate(44_099.998).unwrap(), 44100);
        for rate in [
            0.,
            -1.,
            f64::NAN,
            f64::INFINITY,
            48000.5,
            f64::from(u32::MAX) + 1.,
        ] {
            assert!(integer_rate(rate).is_err());
        }
        assert!(check(0, 8, 8).is_ok());
        assert!(check(-1, 8, 8).is_err());
        assert!(check(0, 4, 8).is_err());
    }
}
