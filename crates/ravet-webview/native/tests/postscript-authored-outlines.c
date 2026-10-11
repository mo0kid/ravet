#include <hb.h>
#include <hb-ot.h>
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_OUTLINE_H
#include FT_MULTIPLE_MASTERS_H
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#define UNUSED (void)funcs;(void)data;(void)state;(void)user
static void hm(hb_draw_funcs_t*funcs,void*data,hb_draw_state_t*state,float x,float y,void*user){UNUSED;printf("M\t%.17g\t%.17g\n",(double)x,(double)y);}
static void hl(hb_draw_funcs_t*funcs,void*data,hb_draw_state_t*state,float x,float y,void*user){UNUSED;printf("L\t%.17g\t%.17g\n",(double)x,(double)y);}
static void hc(hb_draw_funcs_t*funcs,void*data,hb_draw_state_t*state,float x,float y,float a,float b,float c,float d,void*user){UNUSED;printf("C\t%.17g\t%.17g\t%.17g\t%.17g\t%.17g\t%.17g\n",(double)x,(double)y,(double)a,(double)b,(double)c,(double)d);}
static void hz(hb_draw_funcs_t*funcs,void*data,hb_draw_state_t*state,void*user){UNUSED;puts("Z");}
static int fm(const FT_Vector*p,void*data){(void)data;printf("M\t%ld\t%ld\n",p->x,p->y);return 0;}
static int fl(const FT_Vector*p,void*data){(void)data;printf("L\t%ld\t%ld\n",p->x,p->y);return 0;}
static int fq(const FT_Vector*a,const FT_Vector*b,void*data){(void)data;printf("Q\t%ld\t%ld\t%ld\t%ld\n",a->x,a->y,b->x,b->y);return 0;}
static int fc(const FT_Vector*a,const FT_Vector*b,const FT_Vector*c,void*data){(void)data;printf("C\t%ld\t%ld\t%ld\t%ld\t%ld\t%ld\n",a->x,a->y,b->x,b->y,c->x,c->y);return 0;}
int main(int argc,char**argv){
 if(argc<2)return 1;FT_Library library;if(FT_Init_FreeType(&library))return 2;
 FT_Int major=0,minor=0,patch=0;FT_Library_Version(library,&major,&minor,&patch);
 printf("version\tHarfBuzz %s\tFreeType %d.%d.%d\n",hb_version_string(),major,minor,patch);
 hb_draw_funcs_t*draw=hb_draw_funcs_create();hb_draw_funcs_set_move_to_func(draw,hm,NULL,NULL);hb_draw_funcs_set_line_to_func(draw,hl,NULL,NULL);hb_draw_funcs_set_cubic_to_func(draw,hc,NULL,NULL);hb_draw_funcs_set_close_path_func(draw,hz,NULL,NULL);
 for(int i=1;i<argc;++i){
  hb_blob_t*blob=hb_blob_create_from_file_or_fail(argv[i]);if(!blob)return 3;hb_face_t*face=hb_face_create(blob,0);hb_font_t*font=hb_font_create(face);hb_ot_font_set_funcs(font);
  const unsigned upem=hb_face_get_upem(face);hb_font_set_scale(font,(int)upem,(int)upem);
  FT_Face ft;if(FT_New_Face(library,argv[i],0,&ft))return 4;
  const unsigned profiles=hb_ot_var_has_data(face)?4:1;const float users[]={100,400,650,900};
  for(unsigned j=0;j<profiles;++j){
   const float value=profiles==1?0:users[j];if(profiles>1){hb_variation_t v={HB_TAG('w','g','h','t'),value};hb_font_set_variations(font,&v,1);FT_Fixed f=(FT_Fixed)llround((double)value*65536);if(FT_Set_Var_Design_Coordinates(ft,1,&f))return 5;}
   hb_codepoint_t glyph;if(!hb_font_get_nominal_glyph(font,'A',&glyph))return 6;unsigned count=0;const int*coords=hb_font_get_var_coords_normalized(font,&count);
   printf("glyph\tHB\t%s\t%.0f\t%u\t%d\t%d\n",argv[i],(double)value,glyph,hb_font_get_glyph_h_advance(font,glyph),count?coords[0]:0);hb_font_draw_glyph(font,glyph,draw,NULL);puts("end");
   FT_Fixed norm=0;if(profiles>1&&FT_Get_Var_Blend_Coordinates(ft,1,&norm))return 7;
   if(FT_Load_Glyph(ft,glyph,FT_LOAD_NO_SCALE|FT_LOAD_NO_HINTING|FT_LOAD_NO_BITMAP|FT_LOAD_IGNORE_TRANSFORM|FT_LOAD_NO_AUTOHINT))return 8;
   if(ft->glyph->format!=FT_GLYPH_FORMAT_OUTLINE)return 9;
   printf("glyph\tFT\t%s\t%.0f\t%u\t%ld\t%ld\n",argv[i],(double)value,glyph,ft->glyph->metrics.horiAdvance,norm);
   const FT_Outline_Funcs functions={fm,fl,fq,fc,0,0};if(FT_Outline_Decompose(&ft->glyph->outline,&functions,NULL))return 10;puts("end");
  }
  FT_Done_Face(ft);hb_font_destroy(font);hb_face_destroy(face);hb_blob_destroy(blob);
 }
 hb_draw_funcs_destroy(draw);FT_Done_FreeType(library);return 0;
}
