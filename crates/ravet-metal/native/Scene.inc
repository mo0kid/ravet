struct MetalPaint {
  float bounds[4], uv[4], color[4];
  uint32_t image, reserved[3];
};
struct MetalImageSource {
  uint32_t width, height;
  const uint8_t *pixels;
  size_t bytes;
};
static_assert(sizeof(MetalPaint) == 64);
static_assert(sizeof(MetalImageSource) == 24);
extern "C" uint32_t
ravet_metal_render_scene(void *opaque, const MetalPaint *paints, uint32_t count,
                         const MetalImageSource *images, uint32_t imageCount,
                         const float *background, uint8_t *output,
                         size_t capacity) {
  @autoreleasepool {
    auto *context = static_cast<MetalContext *>(opaque);
    if (!context || !background || !output || (count && !paints) ||
        (imageCount && !images) || count > context->maximum ||
        imageCount > 16 ||
        capacity != size_t(context->width) * context->height * 4)
      return 2;
    for (uint32_t i = 0; i < 4; ++i)
      if (!std::isfinite(background[i]) || background[i] < 0 ||
          background[i] > 1)
        return 2;
    size_t uploadBytes = 0;
    for (uint32_t i = 0; i < imageCount; ++i) {
      const auto &image = images[i];
      if (!image.width || !image.height || image.width > 4096 ||
          image.height > 4096 || !image.pixels ||
          image.bytes != size_t(image.width) * image.height * 4 ||
          image.bytes > 67108864 - uploadBytes)
        return 2;
      uploadBytes += image.bytes;
    }
    uint64_t work = uint64_t(context->width) * context->height;
    for (uint32_t i = 0; i < count; ++i) {
      const auto &p = paints[i];
      const auto *b = p.bounds;
      for (float v : p.bounds)
        if (!std::isfinite(v))
          return 2;
      if (b[0] < 0 || b[1] < 0 || b[2] <= b[0] || b[3] <= b[1] ||
          b[2] > context->width || b[3] > context->height ||
          (p.image != UINT32_MAX && p.image >= imageCount))
        return 2;
      for (float v : p.uv)
        if (!std::isfinite(v) || v < 0 || v > 1)
          return 2;
      if (p.uv[2] < p.uv[0] || p.uv[3] < p.uv[1])
        return 2;
      for (float v : p.color)
        if (!std::isfinite(v) || v < 0 || v > 1)
          return 2;
      for (uint32_t reserved : p.reserved)
        if (reserved)
          return 2;
      work += uint64_t(std::ceil(b[2]) - std::floor(b[0])) *
              uint64_t(std::ceil(b[3]) - std::floor(b[1]));
      if (work > 67108864)
        return 2;
    }
    id<MTLTexture> textures[16] = {};
    for (uint32_t i = 0; i < imageCount; ++i) {
      const auto &image = images[i];
      auto *description = [MTLTextureDescriptor
          texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA8Unorm
                                       width:image.width
                                      height:image.height
                                   mipmapped:NO];
      description.storageMode = MTLStorageModeShared;
      description.usage = MTLTextureUsageShaderRead;
      textures[i] = [context->device newTextureWithDescriptor:description];
      if (!textures[i])
        return 1;
      [textures[i]
          replaceRegion:MTLRegionMake2D(0, 0, image.width, image.height)
            mipmapLevel:0
              withBytes:image.pixels
            bytesPerRow:image.width * 4];
    }
    if (count)
      std::memcpy(context->scene.contents, paints, count * sizeof(MetalPaint));
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
    const float dimensions[2] = {float(context->width), float(context->height)};
    [encoder setVertexBytes:dimensions length:sizeof(dimensions) atIndex:1];
    for (uint32_t i = 0; i < count; ++i) {
      const auto image = paints[i].image;
      [encoder setRenderPipelineState:image == UINT32_MAX
                                          ? context->scenePipeline
                                          : context->imagePipeline];
      [encoder setVertexBuffer:context->scene
                        offset:i * sizeof(MetalPaint)
                       atIndex:0];
      if (image != UINT32_MAX)
        [encoder setFragmentTexture:textures[image] atIndex:0];
      [encoder drawPrimitives:MTLPrimitiveTypeTriangle
                  vertexStart:0
                  vertexCount:6];
    }
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
