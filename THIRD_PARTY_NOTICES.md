# Third-Party Components and Compliance Notice

This project uses an FFmpeg binary for recording features via on-demand download. Compliance details are listed below.

## 1. FFmpeg Information

- Component name: FFmpeg
- Copyright: Copyright (c) FFmpeg developers
- Upstream project page: https://ffmpeg.org/
- Upstream source repository: https://git.ffmpeg.org/ffmpeg.git
- Upstream build source used by this
  project: https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-essentials_build.7z
- Distribution strategy in this application: `ffmpeg.exe` is not bundled in the installer. It is downloaded to the local
  `bin` directory when the user enables recording for the first time.
- Default runtime download URL: https://gitee.com/zrq1351/fuyun_tools/releases/download/v0.5.6/ffmpeg.exe
- Config key: `settings.json -> recording_ffmpeg_download_url`

## 2. Corresponding Source Access

- Corresponding source download URL: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz
- Corresponding source version: `ffmpeg 8.0.1`

## 3. License Notes

- FFmpeg itself is generally licensed under LGPL v2.1+.
- The currently used build enables GPL-related components (for example, `libx264`). Distribution of that binary should
  comply with applicable GPL obligations.
- The following license text files should be included in this project/installer package:
    - `GPLv2`
    - `LGPLv2.1`

## 4. Contact

- Compliance contact email: `vzfzhong@gmail.com`
- Response commitment: within 7 business days after receiving a request
