# Demo assets

The cursor silhouette comes from the owner-supplied `cursor-alt-svgrepo-com.svg`
(SVG Repo). `cursor.svg` keeps its path, crops its view box and supplies the
recording colour and outline. The halo and click pulse are rendered by Blinc.

Noto Sans Regular and Bold are unmodified fonts from the Noto Project:

- https://github.com/notofonts/noto-fonts/tree/main/hinted/ttf/NotoSans
- License: `NotoSans-OFL.txt`

These faces match the default Hybrid theme's Noto Sans family. Both native
recordings and the WASM runtime load the same bytes. The published catalog
includes the font license with its source downloads.

JetBrains Mono Regular is copied unchanged from the repository's `assets/fonts`
and loaded alongside Noto Sans for code widgets. Its upstream license is
`JetBrainsMono-OFL.txt` (https://github.com/JetBrains/JetBrainsMono).

`avatar.data-uri` embeds an 840 px JPEG downscale of
`examples/blinc_app_examples/examples/assets/avatar.jpg`; the Image fixture shows it.
`landscape.data-uri` (a JPEG conversion of the Stockcake landscape from
`examples/blinc_app_examples/examples/assets/stormy-plains-illuminated-stockcake.webp`) is no
longer shown. It stays only because every recording's fingerprint lists it; dropping it means
re-recording all previews.
The image and media fixtures do not fetch remote assets.

`handpan.mp4` is a 960 × 540 H.264 conversion of ashui's
`tools/demo/assets/MediaPlayback.mp4`, its original outdoor handpan session.
The H.264 video retains the session's original stereo AAC soundtrack. Live
playback starts with sound after pressing Play; the player controls its volume.

`night-drive.flac` is the first 20 seconds of ashui's original **Night Drive**
by Ash Drive, converted from `apps/dashboard/audio/night-drive.m4a` to mono
22.05 kHz FLAC. `night-drive-waveform.json` contains peak amplitude buckets
from that same excerpt. The native audio fixture starts paused and muted;
playback only starts after a user action. Adjust the volume to listen.

ashui's `apps/dashboard/tools/compose.py` identifies Night Drive as an original
composition generated for that repository. The source project uses Apache 2.0;
its license is included as `ashui-LICENSE.txt`.

`fennec-fox.svg` reuses the detailed vector in
`examples/blinc_app_examples/examples/complex_svg_demo.rs`. Its fill uses
`currentColor` so the gallery can compare sizes and theme-derived tints without
rasterising separate colour variants.
