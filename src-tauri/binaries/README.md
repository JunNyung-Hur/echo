# 동봉 사이드카 바이너리 (ffmpeg / ffprobe)

echo는 녹음 마무리·전사에 ffmpeg/ffprobe를 **별도 프로세스로 호출**한다(라이브러리 링크 X).
릴리스 빌드는 이 폴더의 바이너리를 `externalBin`으로 동봉해, 유저가 ffmpeg를 따로 설치하지
않아도 바로 쓰게 한다. (개발 빌드는 PATH의 ffmpeg로 폴백 — 이 폴더가 비어도 dev는 동작.)

## 라이선스 — 반드시 LGPL 빌드

echo는 오디오만 다루므로(opus 인코딩·wav·webm) GPL 전용 코덱(x264/x265 등)이 필요 없다.
**LGPL 빌드**를 쓰면 echo 코드가 GPL로 전염되지 않고(서브프로세스 호출 = 단순 병합),
배포 의무도 라이선스 텍스트·출처 표기 수준으로 가볍다. GPL 빌드는 소스 제공 의무가 붙으니 쓰지 말 것.

## 받는 법 (Windows x64)

1. BtbN FFmpeg-Builds 릴리스에서 LGPL 아티팩트를 받는다:
   https://github.com/BtbN/FFmpeg-Builds/releases
   → `ffmpeg-master-latest-win64-lgpl.zip` (이름에 **lgpl** 이 들어간 것)
2. zip 안 `bin/ffmpeg.exe`, `bin/ffprobe.exe`를 꺼낸다.
3. 이 폴더에 아래 이름(타깃 트리플 접미사)으로 넣는다:
   - `ffmpeg-x86_64-pc-windows-msvc.exe`
   - `ffprobe-x86_64-pc-windows-msvc.exe`

## 릴리스 설치파일 빌드

동봉(`externalBin`) 설정은 기본 `tauri.conf.json`이 아니라 **릴리스 전용 오버레이**
`src-tauri/tauri.release.conf.json`에 분리해 뒀다. 이렇게 안 하면 tauri-build가 *모든*
빌드(개발용 `cargo build`/`cargo check` 포함)에서 이 바이너리를 요구해, 바이너리 없는
fresh clone에선 개발 빌드가 깨진다.

- **개발 빌드**: 평소대로 (`cargo build`, `npx tauri dev`) — 바이너리 불필요
- **릴리스 설치파일**(ffmpeg 동봉): 저장소 루트에서
  ```
  npx tauri build --config src-tauri/tauri.release.conf.json
  ```
  → 위 두 바이너리가 이 폴더에 있어야 하며, NSIS `.exe` + MSI 가 만들어진다.

(`*.exe`는 .gitignore로 커밋 제외 — 빌드하는 PC에 로컬로 둔다.)
