# Synthetic local-video fixture

`red-blue.mp4` is a public synthetic 64×48, 25 fps two-second H.264/MP4 clip,
red for one second then blue for one second, with a white eight-row top strip. It contains no audio, human image, or
consumer asset/session or third-party media. Regenerate with an explicit local
FFmpeg/OpenH264 build:

```sh
ffmpeg -hide_banner -loglevel error \
  -f lavfi -i color=c=red:s=64x48:r=25:d=1 \
  -f lavfi -i color=c=blue:s=64x48:r=25:d=1 \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0,drawbox=x=0:y=0:w=64:h=8:color=white:t=fill' \
  -c:v libopenh264 -b:v 1000000 -pix_fmt yuv420p -movflags +faststart -y red-blue.mp4
```

No FFmpeg/library dependency is linked or downloaded by RAVET. Tests decode the
prepared file through macOS AVFoundation and compare independent color/time
expectations; codec/OS compatibility beyond that fixture needs separate evidence.
