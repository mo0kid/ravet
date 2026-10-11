#ifndef RAVET_SYSTEM_FONTS_H
#define RAVET_SYSTEM_FONTS_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Private copied ABI. status:0success,1invalid,2capacity,3foreign,4thread,5unavailable,6unsupported,7native. */
typedef struct RavetFontDescription {
 char family[1025],style[1025],display[1025],postscript[1025];
 double weight,width,slant;
 uint32_t traits,availability,format;
} RavetFontDescription;
typedef struct RavetFontAxis {
 uint32_t tag,hidden;
 double minimum,defaultValue,maximum,value;
} RavetFontAxis;
uint32_t ravet_fonts_create(void** destination,size_t* count);
uint32_t ravet_fonts_descriptor(void* owner,size_t index,RavetFontDescription* destination);
uint32_t ravet_fonts_load(void* owner,size_t index,void** destination,size_t* tableCount,size_t* axisCount);
uint32_t ravet_fonts_table(void* resource,size_t index,uint32_t* tag,size_t* length);
uint32_t ravet_fonts_copy_table(void* resource,size_t index,uint8_t* destination,size_t capacity);
uint32_t ravet_fonts_axis(void* resource,size_t index,RavetFontAxis* destination);
void ravet_fonts_resource_destroy(void* resource);
void ravet_fonts_destroy(void* owner);
#ifdef __cplusplus
}
#endif
#endif
