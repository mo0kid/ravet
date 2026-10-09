# Public synthetic codec fixtures

These files contain only two generated sine tones (left 1000 Hz at 0.25 amplitude,
right 500 Hz at 0.5 amplitude), 4096 stereo PCM16 frames at 48000 Hz. No recordings,
product assets, session or licence data are included. The source and fixture data
are first-party RAVET GPL-3.0-only examples.

Regenerate with `python3 scripts/generate-audio-fixtures.py`. The current files
were encoded with local FFmpeg 6.1.1 (FLAC, libmp3lame quality 2, libvorbis quality
5); encoder versions may change bytes. FFmpeg is a fixture-generation tool only,
not a RAVET runtime/build/test dependency. Symphonia supplies optional decoding
under its separate MPL-2.0 licence. Tests compare exact FLAC PCM and lossy tone
quality/channel RMS plus gapless frame count, rather than encoded byte identity.
