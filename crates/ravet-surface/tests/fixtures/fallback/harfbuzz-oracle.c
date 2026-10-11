/* Independent public HarfBuzz full-source contextual segment oracle. */
#include <hb.h>
#include <hb-ot.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc,char** argv) {
    if(argc!=8)return 2;
    size_t length=strlen(argv[2]);unsigned start=(unsigned)strtoul(argv[3],NULL,10),end=(unsigned)strtoul(argv[4],NULL,10);
    if(length>4096||start>end||end>length)return 2;
    hb_blob_t* blob=hb_blob_create_from_file_or_fail(argv[1]);if(!blob)return 3;
    hb_face_t* face=hb_face_create(blob,0);hb_blob_destroy(blob);
    hb_font_t* font=hb_font_create(face);unsigned upem=hb_face_get_upem(face);hb_face_destroy(face);
    hb_ot_font_set_funcs(font);hb_font_set_scale(font,(int)upem,(int)upem);
    hb_buffer_t* buffer=hb_buffer_create();
    hb_buffer_add_utf8(buffer,argv[2],(int)length,start,(int)(end-start));
    hb_buffer_set_direction(buffer,hb_direction_from_string(argv[5],-1));
    hb_buffer_set_script(buffer,hb_script_from_string(argv[6],-1));
    hb_buffer_set_language(buffer,hb_language_from_string(argv[7],-1));
    unsigned flags=HB_BUFFER_FLAG_REMOVE_DEFAULT_IGNORABLES;
    if(start==0)flags|=HB_BUFFER_FLAG_BOT;if(end==length)flags|=HB_BUFFER_FLAG_EOT;
    hb_buffer_set_flags(buffer,(hb_buffer_flags_t)flags);
    hb_shape(font,buffer,NULL,0);
    unsigned count=0;hb_glyph_info_t* infos=hb_buffer_get_glyph_infos(buffer,&count);
    hb_glyph_position_t* positions=hb_buffer_get_glyph_positions(buffer,NULL);
    if(count>16384){hb_buffer_destroy(buffer);hb_font_destroy(font);return 4;}
    printf("{\"upem\":%u,\"glyphs\":[",upem);
    for(unsigned i=0;i<count;++i) {
        if(i)printf(",");
        printf("{\"id\":%u,\"cluster\":%u,\"xo\":%d,\"yo\":%d,\"xa\":%d,\"ya\":%d}",
            infos[i].codepoint,infos[i].cluster,positions[i].x_offset,positions[i].y_offset,positions[i].x_advance,positions[i].y_advance);
    }
    printf("]}\n");hb_buffer_destroy(buffer);hb_font_destroy(font);return 0;
}
