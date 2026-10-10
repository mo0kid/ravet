#import <Metal/Metal.h>
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <new>
struct MetalRectangle {
  float x, y, right, bottom, red, green, blue, alpha;
};
static_assert(sizeof(MetalRectangle) == 32);
struct MetalContext {
  id<MTLDevice> device;
  id<MTLCommandQueue> queue;
  id<MTLRenderPipelineState> pipeline, scenePipeline, imagePipeline;
  id<MTLTexture> texture;
  id<MTLBuffer> rectangles, scene;
  uint32_t width, height, maximum;
};
static NSString *source = @R"METAL(
#include <metal_stdlib>
using namespace metal;
struct Rect { float4 bounds; float4 color; };
struct Vertex { float4 position [[position]]; float4 color; float2 uv; };
struct Paint { float4 bounds; float4 uv; float4 color; uint4 kind; };
vertex Vertex rectangle_vertex(uint index [[vertex_id]],uint instance [[instance_id]],constant Rect *rectangles [[buffer(0)]],constant float2 &size [[buffer(1)]]) {
    const float2 corners[6]={float2(0,0),float2(1,0),float2(0,1),float2(0,1),float2(1,0),float2(1,1)};
    const Rect rectangle=rectangles[instance];
    const float2 pixel=mix(rectangle.bounds.xy,rectangle.bounds.zw,corners[index]);
    Vertex value;value.position=float4(pixel.x/size.x*2-1,1-pixel.y/size.y*2,0,1);
    value.color=float4(rectangle.color.rgb*rectangle.color.a,rectangle.color.a);value.uv=float2(0);return value;
}
vertex Vertex scene_vertex(uint index [[vertex_id]],constant Paint *paints [[buffer(0)]],constant float2 &size [[buffer(1)]]) {
 const float2 corners[6]={float2(0,0),float2(1,0),float2(0,1),float2(0,1),float2(1,0),float2(1,1)};
 const Paint paint=paints[0];const float2 corner=corners[index];
 const float2 pixel=mix(paint.bounds.xy,paint.bounds.zw,corner);
 Vertex value;value.position=float4(pixel.x/size.x*2-1,1-pixel.y/size.y*2,0,1);
 value.color=float4(paint.color.rgb*paint.color.a,paint.color.a);
 value.uv=mix(paint.uv.xy,paint.uv.zw,corner);return value;
}
fragment float4 rectangle_fragment(Vertex input [[stage_in]]) { return input.color; }
fragment float4 image_fragment(Vertex input [[stage_in]],texture2d<float> image [[texture(0)]]) {
 constexpr sampler nearest(coord::normalized,address::clamp_to_edge,filter::nearest);
 return image.sample(nearest,input.uv);
}
)METAL";
extern "C" void *ravet_metal_create(uint32_t width, uint32_t height,
                                    uint32_t maximum, uint32_t *error) {
  @autoreleasepool {
    if (error)
      *error = 1;
    if (!error || !width || !height || width > 4096 || height > 4096 ||
        maximum > 4096)
      return nullptr;
    auto *context = new (std::nothrow) MetalContext;
    if (!context)
      return nullptr;
    context->width = width;
    context->height = height;
    context->maximum = maximum;
    context->device = MTLCreateSystemDefaultDevice();
    if (!context->device) {
      delete context;
      return nullptr;
    }
    context->queue = [context->device newCommandQueue];
    NSError *failure = nil;
    id<MTLLibrary> library = [context->device newLibraryWithSource:source
                                                           options:nil
                                                             error:&failure];
    if (!library) {
      delete context;
      return nullptr;
    }
    MTLRenderPipelineDescriptor *pipeline =
        [[MTLRenderPipelineDescriptor alloc] init];
    pipeline.vertexFunction = [library newFunctionWithName:@"rectangle_vertex"];
    pipeline.fragmentFunction =
        [library newFunctionWithName:@"rectangle_fragment"];
    if (!pipeline.vertexFunction || !pipeline.fragmentFunction) {
      delete context;
      return nullptr;
    }
    auto *color = pipeline.colorAttachments[0];
    color.pixelFormat = MTLPixelFormatRGBA8Unorm;
    color.blendingEnabled = YES;
    color.sourceRGBBlendFactor = MTLBlendFactorOne;
    color.destinationRGBBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    color.sourceAlphaBlendFactor = MTLBlendFactorOne;
    color.destinationAlphaBlendFactor = MTLBlendFactorOneMinusSourceAlpha;
    context->pipeline =
        [context->device newRenderPipelineStateWithDescriptor:pipeline
                                                        error:&failure];
    pipeline.vertexFunction = [library newFunctionWithName:@"scene_vertex"];
    if (!pipeline.vertexFunction) {
      delete context;
      return nullptr;
    }
    context->scenePipeline =
        [context->device newRenderPipelineStateWithDescriptor:pipeline
                                                        error:&failure];
    pipeline.fragmentFunction = [library newFunctionWithName:@"image_fragment"];
    if (!pipeline.fragmentFunction) {
      delete context;
      return nullptr;
    }
    context->imagePipeline =
        [context->device newRenderPipelineStateWithDescriptor:pipeline
                                                        error:&failure];
    context->scene =
        [context->device newBufferWithLength:std::max(1u, maximum) * 64
                                     options:MTLResourceStorageModeShared];
    auto *texture = [MTLTextureDescriptor
        texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                     width:width
                                    height:height
                                 mipmapped:NO];
    texture.storageMode = MTLStorageModeShared;
    texture.usage = MTLTextureUsageRenderTarget;
    context->texture = [context->device newTextureWithDescriptor:texture];
    context->rectangles = [context->device
        newBufferWithLength:std::max(1u, maximum) * sizeof(MetalRectangle)
                    options:MTLResourceStorageModeShared];
    if (!context->queue || !context->pipeline || !context->scenePipeline ||
        !context->imagePipeline || !context->scene || !context->texture ||
        !context->rectangles) {
      delete context;
      return nullptr;
    }
    *error = 0;
    return context;
  }
}
extern "C" uint32_t ravet_metal_render(void *opaque,
                                       const MetalRectangle *rectangles,
                                       uint32_t count, const float *background,
                                       uint8_t *output, size_t capacity) {
  @autoreleasepool {
    auto *context = static_cast<MetalContext *>(opaque);
    if (!context || !background || !output || (count && !rectangles) ||
        count > context->maximum ||
        capacity != size_t(context->width) * context->height * 4)
      return 2;
    for (uint32_t i = 0; i < 4; ++i)
      if (!std::isfinite(background[i]) || background[i] < 0 ||
          background[i] > 1)
        return 2;
    uint64_t work = uint64_t(context->width) * context->height;
    for (uint32_t i = 0; i < count; ++i) {
      const auto &r = rectangles[i];
      if (!std::isfinite(r.x) || !std::isfinite(r.y) ||
          !std::isfinite(r.right) || !std::isfinite(r.bottom) || r.x < 0 ||
          r.y < 0 || r.right <= r.x || r.bottom <= r.y ||
          r.right > context->width || r.bottom > context->height)
        return 2;
      for (float value : {r.red, r.green, r.blue, r.alpha})
        if (!std::isfinite(value) || value < 0 || value > 1)
          return 2;
      work += uint64_t(std::ceil(r.right) - std::floor(r.x)) *
              uint64_t(std::ceil(r.bottom) - std::floor(r.y));
      if (work > 67108864)
        return 2;
    }
    if (count)
      std::memcpy(context->rectangles.contents, rectangles,
                  count * sizeof(MetalRectangle));
    auto *pass = [MTLRenderPassDescriptor renderPassDescriptor];
    pass.colorAttachments[0].texture = context->texture;
    pass.colorAttachments[0].loadAction = MTLLoadActionClear;
    pass.colorAttachments[0].storeAction = MTLStoreActionStore;
    pass.colorAttachments[0].clearColor = MTLClearColorMake(
        background[0] * background[3], background[1] * background[3],
        background[2] * background[3], background[3]);
    id<MTLCommandBuffer> command = [context->queue commandBuffer];
    id<MTLRenderCommandEncoder> encoder =
        [command renderCommandEncoderWithDescriptor:pass];
    if (!command || !encoder)
      return 1;
    [encoder setRenderPipelineState:context->pipeline];
    [encoder setVertexBuffer:context->rectangles offset:0 atIndex:0];
    const float dimensions[2] = {float(context->width), float(context->height)};
    [encoder setVertexBytes:dimensions length:sizeof(dimensions) atIndex:1];
    if (count)
      [encoder drawPrimitives:MTLPrimitiveTypeTriangle
                  vertexStart:0
                  vertexCount:6
                instanceCount:count];
    [encoder endEncoding];
    [command commit];
    [command waitUntilCompleted];
    if (command.status != MTLCommandBufferStatusCompleted)
      return 1;
    [context->texture
           getBytes:output
        bytesPerRow:context->width * 4
         fromRegion:MTLRegionMake2D(0, 0, context->width, context->height)
        mipmapLevel:0];
    return 0;
  }
}
extern "C" void ravet_metal_close(void *opaque) {
  @autoreleasepool {
    delete static_cast<MetalContext *>(opaque);
  }
}

#include "Scene.inc"
