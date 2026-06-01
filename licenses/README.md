# Bundled third-party license texts

License texts for components bundled into echo's distributed installer.
See `../THIRD-PARTY-NOTICES.md` for the full attribution.

## FFmpeg — to add: `FFmpeg-LICENSE.txt`

The bundled FFmpeg (LGPL build) ships its own license text inside the release
archive you downloaded. Copy that file here so it travels with the distribution:

1. In the FFmpeg build zip (e.g. `ffmpeg-master-latest-win64-lgpl.zip` from
   https://github.com/BtbN/FFmpeg-Builds/releases), find the `LICENSE` (or
   `LICENSE.txt` / `COPYING`) file — typically at the archive root or under `doc/`.
2. Copy it here as **`FFmpeg-LICENSE.txt`** and commit it (it's small text, kept
   in the repo for license compliance — unlike the `.exe` binaries).

The canonical LGPL v3 text is also at https://www.gnu.org/licenses/lgpl-3.0.txt.
