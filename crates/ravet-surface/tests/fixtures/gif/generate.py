"""First-party patterns; Pillow independently writes GIF and expected RGBA."""
from pathlib import Path
from PIL import Image
import hashlib
import json

root = Path(__file__).parent
root.mkdir(exist_ok=True)
records = []
for name, width, height, transparency, interlace in [
    ('opaque', 7, 5, None, False),
    ('transparent', 9, 3, 2, False),
    ('interlaced', 37, 23, None, True),
]:
    image = Image.new('P', (width, height))
    image.putpalette([c for i in range(256) for c in [(i * 17) % 256, (i * 37) % 256, (i * 61) % 256]])
    image.putdata([(x * 3 + y * 5) % 16 for y in range(height) for x in range(width)])
    options = dict(optimize=False, interlace=interlace)
    if transparency is not None:
        options['transparency'] = transparency
    path = root / (name + '.gif')
    image.save(path, **options)
    with Image.open(path) as decoded:
        rgba = decoded.convert('RGBA').tobytes()
        assert decoded.n_frames == 1
    (root / (name + '.rgba')).write_bytes(rgba)
    records.append(dict(name=name, width=width, height=height, transparency=transparency,
        interlace=interlace, sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
        rgba_sha256=hashlib.sha256(rgba).hexdigest()))
(root / 'provenance.json').write_text(json.dumps(records, indent=2) + '\n')
