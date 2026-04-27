# Third-Party Components and Compliance Notice

This project uses third-party components, including FFmpeg and OpenCV. This file documents distribution and license
compliance information for releases.

## 1. FFmpeg Distribution Information

- Component: FFmpeg
- Copyright: Copyright (c) FFmpeg developers
- Upstream project page: https://ffmpeg.org/
- Upstream source repository: https://git.ffmpeg.org/ffmpeg.git
- Upstream build package referenced by this
  project: https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-essentials_build.7z
- Distribution strategy: `ffmpeg.exe` is not bundled in the installer; it is downloaded to local `bin` directory when
  recording is enabled.
- Default runtime download URL: https://gitee.com/zrq1351/fuyun_tools/releases/download/v0.5.6/ffmpeg.exe
- Config key: `settings.json -> recording_ffmpeg_download_url`

## 2. FFmpeg License and Scope

- FFmpeg contains components under LGPL v2.1+ and may include GPL components depending on build options.
- The binary distributed by this project is treated as a GPL-compliant FFmpeg distribution (for example, with `libx264`
  enabled).
- License scope clarification:
    - FFmpeg binary (`ffmpeg.exe`) follows its own upstream licenses.
    - This desktop application invokes FFmpeg as an external process.
    - Distribution obligations for FFmpeg are handled as third-party binary compliance obligations.

## 3. Corresponding Source Access (FFmpeg)

- Corresponding source version: `ffmpeg 8.0.1`
- Upstream corresponding source URL: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz
- Upstream source repository: https://git.ffmpeg.org/ffmpeg.git
- Build reference package URL: https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.0.1-essentials_build.7z
- Availability commitment: source access information for distributed FFmpeg binaries will be kept available for at least
  3 years from release date.
- If a release distributes a modified FFmpeg binary, the release page must additionally provide the modified
  corresponding source (including build scripts/config used for that binary).

## 4. Required License Texts in Distribution

The following files must be included in application package/installer or an equivalent accessible location:

- `GPLv2` (for FFmpeg GPL components)
- `LGPLv2.1` (for FFmpeg LGPL components)
- `Apache-2.0` (for PaddleOCR, MNN, and OpenCV)
- `THIRD_PARTY_NOTICES.md` (this file)
- `OpenCV_LICENSE` (when OpenCV-related build is distributed)

## 5. Compliance Contact

- Contact email: `vzfzhong@gmail.com`
- Response target: within 7 business days after receiving a compliance request

## 6. OpenCV 4 Information

- Component: OpenCV
- Version in current build chain: `4.12.0` (via vcpkg `opencv4:x64-windows-static`)
- Upstream project page: https://opencv.org/
- Upstream source repository: https://github.com/opencv/opencv
- License: Apache License 2.0 (with bundled third-party notices included in distributed OpenCV license file)
- Distribution in this application: OpenCV is linked for the `longshot-opencv` desktop feature.
- Included license file in package: `OpenCV_LICENSE`

## 7. PaddleOCR Models (PP-OCRv5)

- Component: PaddleOCR PP-OCRv5 Models
- Version: PP-OCRv5 (via ocr-rs library)
- Upstream project page: https://github.com/PaddlePaddle/PaddleOCR
- Upstream documentation: https://paddlepaddle.github.io/PaddleOCR/
- License: Apache License 2.0
- Models used in this application:
  - `PP-OCRv5_mobile_det.mnn` - Text detection model (MNN format)
  - `PP-OCRv5_mobile_rec.mnn` - Text recognition model (MNN format)
  - `ppocr_keys_v5.txt` - Character dictionary for recognition
- Distribution in this application: Bundled in `models` directory for offline OCR text recognition via ocr-rs library
- Attribution requirement: Include Apache 2.0 license notice and copyright statement in distribution
- Copyright: Copyright (c) 2020-2026 PaddlePaddle Authors. All Rights Reserved.

## 8. MNN Inference Engine

- Component: MNN (Mobile Neural Network)
- Upstream project page: https://www.mnn.zone/
- Upstream source repository: https://github.com/alibaba/MNN
- License: Apache License 2.0
- Distribution in this application: Used as inference engine for PaddleOCR models, built from source via ocr-rs library with `build-mnn-from-source` feature
- Attribution requirement: Include Apache 2.0 license notice and copyright statement in distribution
- Copyright: Copyright (c) Alibaba Group Holding Limited. All Rights Reserved.

## 9. ocr-rs Library

- Component: ocr-rs
- Version: 2.2.2 (via crates.io)
- Upstream repository: https://crates.io/crates/ocr-rs
- License: Check upstream repository for exact license (typically MIT or Apache 2.0)
- Distribution in this application: Rust library providing OCR functionality by integrating PaddleOCR models with MNN inference engine
- Features used: `build-mnn-from-source`
