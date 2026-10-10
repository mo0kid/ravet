#import <AVFoundation/AVFoundation.h>
#import <CoreGraphics/CoreGraphics.h>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <new>
// macOS12 synchronous worker entry points; never an audio callback API.
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
struct VideoInfo {
  double duration;
  uint32_t width, height;
};
struct VideoHandle {
  AVURLAsset *asset;
  AVAssetImageGenerator *generator;
  VideoInfo info;
};
extern "C" void *ravet_video_open(const char *path, VideoInfo *info,
                                  uint32_t *error) {
  @autoreleasepool {
    if (error)
      *error = 1;
    if (!path || !info || !error)
      return nullptr;
    NSString *name = [[NSString alloc] initWithUTF8String:path];
    if (!name)
      return nullptr;
    NSURL *url = [NSURL fileURLWithPath:name];
    AVURLAsset *asset =
        [AVURLAsset URLAssetWithURL:url
                            options:@{
                              AVURLAssetPreferPreciseDurationAndTimingKey : @YES
                            }];
    NSArray<AVAssetTrack *> *tracks =
        [asset tracksWithMediaType:AVMediaTypeVideo];
    const double duration = CMTimeGetSeconds(asset.duration);
    if (tracks.count == 0 || !std::isfinite(duration) || duration <= 0.) {
      *error = 3;
      return nullptr;
    }
    AVAssetTrack *track = tracks.firstObject;
    const CGRect transformed = CGRectApplyAffineTransform(
        CGRectMake(0, 0, track.naturalSize.width, track.naturalSize.height),
        track.preferredTransform);
    const double width = std::ceil(std::abs(transformed.size.width)),
                 height = std::ceil(std::abs(transformed.size.height));
    if (!std::isfinite(width) || !std::isfinite(height) || width < 1 ||
        height < 1 || width > 4096 || height > 4096 || duration > 86400.) {
      *error = 4;
      return nullptr;
    }
    auto *handle = new (std::nothrow) VideoHandle;
    if (!handle) {
      *error = 3;
      return nullptr;
    }
    handle->asset = asset;
    handle->generator =
        [AVAssetImageGenerator assetImageGeneratorWithAsset:asset];
    handle->generator.appliesPreferredTrackTransform = YES;
    handle->generator.requestedTimeToleranceBefore = kCMTimeZero;
    handle->generator.requestedTimeToleranceAfter = kCMTimeZero;
    handle->generator.maximumSize = CGSizeMake(4096, 4096);
    if (!handle->generator) {
      delete handle;
      *error = 3;
      return nullptr;
    }
    handle->info = {duration, uint32_t(width), uint32_t(height)};
    *info = handle->info;
    *error = 0;
    return handle;
  }
}
extern "C" uint32_t ravet_video_frame(void *opaque, double seconds,
                                      uint8_t *output, size_t capacity,
                                      VideoInfo *info, double *actual) {
  @autoreleasepool {
    auto *handle = static_cast<VideoHandle *>(opaque);
    if (!handle || !output || !info || !actual || !std::isfinite(seconds) ||
        seconds < 0 || seconds >= handle->info.duration)
      return 1;
    CMTime timestamp{};
    NSError *error = nil;
    CGImageRef image = [handle->generator
        copyCGImageAtTime:CMTimeMakeWithSeconds(seconds, 1000000)
               actualTime:&timestamp
                    error:&error];
    if (!image)
      return 3;
    const size_t width = CGImageGetWidth(image),
                 height = CGImageGetHeight(image);
    const double time = CMTimeGetSeconds(timestamp);
    if (!width || !height || width > 4096 || height > 4096 ||
        width * height * 4 > capacity || !std::isfinite(time) || time < 0 ||
        time >= handle->info.duration) {
      CGImageRelease(image);
      return 4;
    }
    CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef context =
        space ? CGBitmapContextCreate(
                    output, width, height, 8, width * 4, space,
                    kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Big)
              : nullptr;
    if (space)
      CGColorSpaceRelease(space);
    if (!context) {
      CGImageRelease(image);
      return 3;
    }
    std::memset(output, 0, width * height * 4);
    CGContextSetBlendMode(context, kCGBlendModeCopy);
    CGContextDrawImage(context, CGRectMake(0, 0, width, height), image);
    CGContextRelease(context);
    CGImageRelease(image);
    *info = {handle->info.duration, uint32_t(width), uint32_t(height)};
    *actual = time;
    return 0;
  }
}
extern "C" void ravet_video_close(void *opaque) {
  @autoreleasepool {
    delete static_cast<VideoHandle *>(opaque);
  }
}
