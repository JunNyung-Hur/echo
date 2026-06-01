# Third-party notices

echo itself is licensed under the Apache License 2.0 (see `LICENSE`). It bundles
the following third-party component in its distributed installer.

---

## FFmpeg

- **Project:** FFmpeg — https://ffmpeg.org
- **License:** GNU Lesser General Public License, version 3 (LGPL v3) —
  https://www.gnu.org/licenses/lgpl-3.0.html (which incorporates the GNU GPL v3,
  https://www.gnu.org/licenses/gpl-3.0.html)
- **How it's used:** echo invokes `ffmpeg` / `ffprobe` as **separate processes**
  (via the command line) to convert and probe audio. FFmpeg is **not** linked into
  echo and is **not** modified. Under the LGPL/GPL, command-line/subprocess use is
  mere aggregation — echo's own code (Apache 2.0) is unaffected.
- **Build:** an LGPL build (LGPL components only — no `--enable-gpl`, no
  `--enable-nonfree`; GPL codecs such as libx264/libx265 are disabled). Prebuilt
  LGPL Windows binaries are published at https://github.com/BtbN/FFmpeg-Builds
  (the `*-win64-lgpl` artifact).
- **Corresponding source:** FFmpeg source is available from
  https://ffmpeg.org/download.html and the build project above.

### License text

The exact license text shipped with the bundled FFmpeg build (the `LICENSE` /
`COPYING` file inside the FFmpeg release archive) is included alongside the
bundled binary as `licenses/FFmpeg-LICENSE.txt`. The canonical LGPL v3 text is at
the URL above.

> The `ffmpeg`/`ffprobe` binaries themselves are not committed to this repository
> (they are large and fetched locally for release builds — see
> `src-tauri/binaries/README.md`); they are embedded into the distributed
> installer.
