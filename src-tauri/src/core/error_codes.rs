use serde::{Deserialize, Serialize};
use crate::core::error::ErrorCode;

/// 机器可读的错误码标识
/// 前端根据此错误码查找对应的 i18n 翻译
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppErrorKind {
    // === 通用 ===
    Unknown,
    InternalError,
    TaskExecutionFailed,

    // === 设置校验 ===
    SettingsMaxItemsRange,
    SettingsTextMaxItemsRange,
    SettingsImageMaxItemsRange,
    SettingsImageDiskLimitRange,
    SettingsImageFillVerifyModeInvalid,
    SettingsHotkeyFormatInvalid,
    SettingsHotkeyEmpty,
    SettingsHotkeyConflict,
    SettingsHotkeysIdentical,
    SettingsRecordingFpsRange,
    SettingsRecordingVideoBitrateRange,
    SettingsRecordingAudioBitrateRange,
    SettingsRecordingMaxDurationRange,
    SettingsRecordingFileNameEmpty,
    SettingsRecordingFfmpegUrlEmpty,
    SettingsRecordingFfmpegUrlNotHttps,
    SettingsRecordingAudioSyncRange,
    SettingsClipboardBottomOffsetRange,
    SettingsTranslationPromptEmpty,
    SettingsExplanationPromptEmpty,
    SettingsTranslationPromptMissingPlaceholder,
    SettingsExplanationPromptMissingPlaceholder,
    SettingsProviderNameEmpty,
    SettingsSaveProviderFailed,
    SettingsSaveFailed,
    SettingsValidationFailed,
    SettingsApiKeySaveFailed,
    SettingsApiKeyGetFailed,
    SettingsCredentialCreateFailed,
    SettingsLocalKeyNotFound,

    // === AI 服务 ===
    AiNotConfigured,
    AiProviderNotFound,
    AiApiUrlEmpty,
    AiModelNameEmpty,
    AiApiUrlInvalid,
    AiApiKeyNotConfigured,
    AiKeychainReadFailed,
    AiClientInitFailed,
    AiConnectionTestFailed,
    AiConnectionTestNoResponse,
    AiTextEmpty,

    // === 剪贴板 ===
    ClipboardHotkeyRegisterFailed,
    ClipboardCategoryAddFailed,
    ClipboardCategoryRemoveFailed,
    ClipboardCategorySetFailed,
    ClipboardDeleteTextFailed,
    ClipboardDeleteImageFailed,
    ClipboardItemNotFound,
    ClipboardPinFailed,
    ClipboardPinImageFailed,
    ClipboardWarmupFailed,
    ClipboardPreviewPathFailed,
    ClipboardPreviewShowFailed,
    ClipboardImagePathEmpty,
    ClipboardImageFileNotFound,
    ClipboardImageFormatUnsupported,
    ClipboardSetTagsFailed,
    ClipboardUpdateContentFailed,
    ClipboardSetPinFailed,
    ClipboardCleanTextFailed,
    ClipboardCleanImageFailed,
    ClipboardNoFilesSelected,
    ClipboardNoImagesFound,
    ClipboardNoImagesImported,
    ClipboardImportFailed,
    ClipboardCopyTextFailed,
    ClipboardAutoPasteFailed,

    // === 截图 ===
    ScreenshotSourceFileNotFound,
    ScreenshotTargetDirEmpty,
    ScreenshotSavePathEmpty,
    ScreenshotUnsupportedOperation,
    ScreenshotFeatureDisabled,
    ScreenshotFailed,
    ScreenshotWriteSourceFailed,
    ScreenshotCreateWindowFailed,
    ScreenshotLongshotStatusFailed,
    LongshotAreaTooSmall,
    LongshotAlreadyRunning,
    LongshotSessionNotFound,
    LongshotSessionIdMismatch,
    LongshotNoValidCapture,
    LongshotNoSegments,
    LongshotResultEmpty,
    LongshotFrameTooSmall,
    LongshotAreaTooLarge,
    LongshotDependencyMissing,
    LongshotFfmpegReadFailed,
    LongshotCancelled,

    // === 录屏 ===
    RecordingFeatureDisabled,
    RecordingFfmpegNotFound,
    RecordingStartFailed,
    RecordingStopFailed,
    RecordingPauseFailed,
    RecordingResumeFailed,
    RecordingWindowInvalid,
    RecordingWindowInvisible,
    RecordingWindowMinimized,

    // === 备份 ===
    BackupDirNotConfigured,
    BackupDirNotSet,
    BackupInvalidFile,
    BackupDeleteOutsideDir,

    // === 文档管理 ===
    DocumentDirHasFiles,
    DocumentCategoryHasFiles,
    DocumentPathNotDir,
    DocumentCategoryNameEmpty,
    DocumentCategoryNameInvalidChar,
    DocumentFileNotFound,
    DocumentMoveFailed,
    DocumentDeleteFailed,
    DocumentRenameFailed,
    DocumentScanFailed,
    DocumentImportFailed,
    DocumentDetectionFailed,
    DocumentDirNotFound,

    // === 启动器 ===
    LauncherStartupFailed,
    LauncherCommandPrefixExists,
    LauncherCommandNotFound,
    LauncherShortcutNotFound,
    LauncherShortcutResolveFailed,
    LauncherAppDirFailed,
    LauncherNotWindows,

    // === 划词 ===
    SelectionFeatureDisabled,
    SelectionClipboardDisabled,
    SelectionImageClipboardDisabled,
    SelectionCtrlReleaseFailed,
    SelectionWaitHideTimeout,

    // === VC 运行库 ===
    VcRuntimeDownloadUrlEmpty,
    VcRuntimeDownloadUrlSha256Invalid,

    // === 图片存储 / 数据库 ===
    ImageStoreReadFailed,
    ImageStoreWriteFailed,
    ImageStoreInitFailed,
    ImageStorePoolFailed,
    ImageStoreBatchDeleteFailed,
    DatabaseTargetNotFound,
    DatabaseError,
    IoError,
    JsonError,

    // === 系统 ===
    SystemImageDataEmpty,
    SystemLocalImageEmpty,
    SystemIndexOutOfRange,
    SystemUnsupportedCleanMode,
    SystemPreviewGenerating,
    SystemWebImageNoData,
    SystemClipboardNotBitmap,
    SystemWriteClipboardFailed,
}

impl AppErrorKind {
    /// 默认中文消息模板
    pub fn default_message(&self) -> &'static str {
        match self {
            Self::Unknown => "未知错误",
            Self::InternalError => "内部错误",
            Self::TaskExecutionFailed => "任务执行失败",

            Self::SettingsMaxItemsRange => "max_items必须在1-1000之间",
            Self::SettingsTextMaxItemsRange => "text_max_items必须在1-1000之间",
            Self::SettingsImageMaxItemsRange => "image_max_items必须在1-1000之间",
            Self::SettingsImageDiskLimitRange => "image_disk_limit_mb必须在100-102400之间",
            Self::SettingsImageFillVerifyModeInvalid => "image_fill_verify_mode必须是strict或fast",
            Self::SettingsHotkeyFormatInvalid => "快捷键格式无效，必须包含修饰键",
            Self::SettingsHotkeyEmpty => "快捷键不能为空",
            Self::SettingsHotkeyConflict => "快捷键被占用",
            Self::SettingsHotkeysIdentical => "快捷键不能相同",
            Self::SettingsRecordingFpsRange => "recording_default_fps必须在1-120之间",
            Self::SettingsRecordingVideoBitrateRange => "recording_default_video_bitrate_kbps必须在500-50000之间",
            Self::SettingsRecordingAudioBitrateRange => "recording_default_audio_bitrate_kbps必须在32-512之间",
            Self::SettingsRecordingMaxDurationRange => "recording_max_duration_minutes必须在1-1440之间",
            Self::SettingsRecordingFileNameEmpty => "recording_file_name_template不能为空",
            Self::SettingsRecordingFfmpegUrlEmpty => "recording_ffmpeg_download_url不能为空",
            Self::SettingsRecordingFfmpegUrlNotHttps => "recording_ffmpeg_download_url必须以https://开头",
            Self::SettingsRecordingAudioSyncRange => "recording_window_audio_sync_advance_ms必须在0-500之间",
            Self::SettingsClipboardBottomOffsetRange => "clipboard_bottom_offset必须在0-400之间",
            Self::SettingsTranslationPromptEmpty => "翻译提示模板不能为空",
            Self::SettingsExplanationPromptEmpty => "解释提示模板不能为空",
            Self::SettingsTranslationPromptMissingPlaceholder => "翻译提示模板必须包含{text}和{target_language}占位符",
            Self::SettingsExplanationPromptMissingPlaceholder => "解释提示模板必须包含{text}和{target_language}占位符",
            Self::SettingsProviderNameEmpty => "提供商名称不能为空",
            Self::SettingsSaveProviderFailed => "保存提供商配置失败",
            Self::SettingsSaveFailed => "保存设置失败",
            Self::SettingsValidationFailed => "设置验证失败",
            Self::SettingsApiKeySaveFailed => "保存API密钥失败",
            Self::SettingsApiKeyGetFailed => "获取API密钥失败",
            Self::SettingsCredentialCreateFailed => "创建凭据入口失败",
            Self::SettingsLocalKeyNotFound => "未能获取到真实的 API 密钥",

            Self::AiNotConfigured => "未配置AI提供商，请在设置中选择提供商",
            Self::AiProviderNotFound => "未找到提供商的配置，请在设置中配置API信息",
            Self::AiApiUrlEmpty => "API地址不能为空，请在设置中填写正确的API地址",
            Self::AiModelNameEmpty => "模型名称不能为空，请在设置中填写正确的模型名称",
            Self::AiApiUrlInvalid => "API地址格式不正确",
            Self::AiApiKeyNotConfigured => "API密钥未配置或无效，请在设置中填写正确的API密钥",
            Self::AiKeychainReadFailed => "读取密钥库失败",
            Self::AiClientInitFailed => "客户端初始化失败",
            Self::AiConnectionTestFailed => "连接测试失败",
            Self::AiConnectionTestNoResponse => "连接测试未返回预期结果",
            Self::AiTextEmpty => "文本为空，无法处理",

            Self::ClipboardHotkeyRegisterFailed => "快捷键被占用或注册失败",
            Self::ClipboardCategoryAddFailed => "新增分类失败",
            Self::ClipboardCategoryRemoveFailed => "删除分类失败",
            Self::ClipboardCategorySetFailed => "设置分类失败",
            Self::ClipboardDeleteTextFailed => "删除文本历史失败",
            Self::ClipboardDeleteImageFailed => "删除图片历史失败",
            Self::ClipboardItemNotFound => "找不到目标项目",
            Self::ClipboardPinFailed => "置顶文本失败",
            Self::ClipboardPinImageFailed => "置顶图片失败",
            Self::ClipboardWarmupFailed => "预热图片失败",
            Self::ClipboardPreviewPathFailed => "获取预览图片路径失败",
            Self::ClipboardPreviewShowFailed => "显示图片预览失败",
            Self::ClipboardImagePathEmpty => "图片路径为空",
            Self::ClipboardImageFileNotFound => "图片文件不存在",
            Self::ClipboardImageFormatUnsupported => "不支持的图片格式",
            Self::ClipboardSetTagsFailed => "设置图片标签失败",
            Self::ClipboardUpdateContentFailed => "更新文本内容失败",
            Self::ClipboardSetPinFailed => "设置置顶状态失败",
            Self::ClipboardCleanTextFailed => "清理文本历史失败",
            Self::ClipboardCleanImageFailed => "清理图片历史失败",
            Self::ClipboardNoFilesSelected => "未选择任何文件或文件夹",
            Self::ClipboardNoImagesFound => "未找到可导入的图片",
            Self::ClipboardNoImagesImported => "未导入任何图片",
            Self::ClipboardImportFailed => "导入图片失败",
            Self::ClipboardCopyTextFailed => "复制文本失败",
            Self::ClipboardAutoPasteFailed => "自动粘贴失败",

            Self::ScreenshotSourceFileNotFound => "源图片文件不存在",
            Self::ScreenshotTargetDirEmpty => "目标目录不能为空",
            Self::ScreenshotSavePathEmpty => "保存路径为空",
            Self::ScreenshotUnsupportedOperation => "不支持的长截图操作",
            Self::ScreenshotFeatureDisabled => "截图功能已停用",
            Self::ScreenshotFailed => "截图失败",
            Self::ScreenshotWriteSourceFailed => "写入截图源图失败",
            Self::ScreenshotCreateWindowFailed => "创建截图窗口失败",
            Self::ScreenshotLongshotStatusFailed => "长截图状态获取失败",
            Self::LongshotAreaTooSmall => "长截图区域太小，最小为 64x64",
            Self::LongshotAlreadyRunning => "已有长截图会话正在运行，请先完成或取消",
            Self::LongshotSessionNotFound => "未找到进行中的长截图会话",
            Self::LongshotSessionIdMismatch => "长截图会话 ID 不匹配",
            Self::LongshotNoValidCapture => "长截图没有采集到有效画面",
            Self::LongshotNoSegments => "长截图没有可拼接分段",
            Self::LongshotResultEmpty => "长截图结果为空",
            Self::LongshotFrameTooSmall => "长截图帧尺寸过小",
            Self::LongshotAreaTooLarge => "长截图区域尺寸过大",
            Self::LongshotDependencyMissing => "长截图依赖未就绪，请检查 FFmpeg 或 OpenCV 环境",
            Self::LongshotFfmpegReadFailed => "无法读取 ffmpeg 输出",
            Self::LongshotCancelled => "长截图已取消",

            Self::RecordingFeatureDisabled => "录屏功能已停用",
            Self::RecordingFfmpegNotFound => "未找到ffmpeg",
            Self::RecordingStartFailed => "开始录制失败",
            Self::RecordingStopFailed => "停止录制失败",
            Self::RecordingPauseFailed => "暂停录制失败",
            Self::RecordingResumeFailed => "恢复录制失败",
            Self::RecordingWindowInvalid => "目标窗口句柄已失效或窗口已关闭",
            Self::RecordingWindowInvisible => "目标窗口当前不可见，请将窗口切回前台后重试",
            Self::RecordingWindowMinimized => "目标窗口已最小化，请恢复窗口后再开始录制",

            Self::BackupDirNotConfigured => "未配置备份目录",
            Self::BackupDirNotSet => "请先配置自动备份目录",
            Self::BackupInvalidFile => "仅允许删除 .fytbk.zip 备份文件",
            Self::BackupDeleteOutsideDir => "禁止删除备份目录之外的文件",

            Self::DocumentDirHasFiles => "该目录下存在文件，请先将文件删除或移至其他目录后再删除",
            Self::DocumentCategoryHasFiles => "该分类下存在文件，请先将文件移至其他分类或取消分类后再删除",
            Self::DocumentPathNotDir => "路径不是一个目录",
            Self::DocumentCategoryNameEmpty => "分类名称不能为空",
            Self::DocumentCategoryNameInvalidChar => "分类名称包含无效字符",
            Self::DocumentFileNotFound => "文件不存在",
            Self::DocumentMoveFailed => "移动文件失败",
            Self::DocumentDeleteFailed => "删除文件失败",
            Self::DocumentRenameFailed => "重命名失败",
            Self::DocumentScanFailed => "扫描失败",
            Self::DocumentImportFailed => "导入失败",
            Self::DocumentDetectionFailed => "检测失败",
            Self::DocumentDirNotFound => "目录不存在",

            Self::LauncherStartupFailed => "启动失败",
            Self::LauncherCommandPrefixExists => "命令前缀已存在",
            Self::LauncherCommandNotFound => "命令不存在",
            Self::LauncherShortcutNotFound => "快捷方式不存在",
            Self::LauncherShortcutResolveFailed => "无法解析快捷方式目标",
            Self::LauncherAppDirFailed => "无法获取应用目录",
            Self::LauncherNotWindows => "非 Windows 平台不支持此操作",

            Self::SelectionFeatureDisabled => "划词功能已禁用",
            Self::SelectionClipboardDisabled => "剪贴板功能已禁用",
            Self::SelectionImageClipboardDisabled => "图片剪贴板功能已禁用",
            Self::SelectionCtrlReleaseFailed => "释放 Ctrl 键失败",
            Self::SelectionWaitHideTimeout => "等待窗口隐藏超时",

            Self::VcRuntimeDownloadUrlEmpty => "下载地址不能为空",
            Self::VcRuntimeDownloadUrlSha256Invalid => "下载地址中的 sha256 参数格式无效",

            Self::ImageStoreReadFailed => "读取图片历史数据库失败",
            Self::ImageStoreWriteFailed => "写入图片历史数据库失败",
            Self::ImageStoreInitFailed => "初始化图片历史数据库失败",
            Self::ImageStorePoolFailed => "创建数据库连接池失败",
            Self::ImageStoreBatchDeleteFailed => "批量删除图片项失败",
            Self::DatabaseTargetNotFound => "目标记录不存在",
            Self::DatabaseError => "数据库错误",
            Self::IoError => "文件系统错误",
            Self::JsonError => "JSON 解析错误",

            Self::SystemImageDataEmpty => "图片数据为空",
            Self::SystemLocalImageEmpty => "本地图片为空",
            Self::SystemIndexOutOfRange => "索引超出范围",
            Self::SystemUnsupportedCleanMode => "不支持的清理模式",
            Self::SystemPreviewGenerating => "预览正在生成中",
            Self::SystemWebImageNoData => "检测到网页图片链接，但剪贴板中没有位图数据",
            Self::SystemClipboardNotBitmap => "当前剪贴板不是位图格式",
            Self::SystemWriteClipboardFailed => "写入剪贴板图片失败",
        }
    }

    /// 返回错误所属的分类
    pub fn category(&self) -> ErrorCode {
        use crate::core::error::ErrorCode;
        match self {
            Self::Unknown | Self::InternalError | Self::TaskExecutionFailed => ErrorCode::SystemError,
            Self::SettingsMaxItemsRange
            | Self::SettingsTextMaxItemsRange
            | Self::SettingsImageMaxItemsRange
            | Self::SettingsImageDiskLimitRange
            | Self::SettingsImageFillVerifyModeInvalid
            | Self::SettingsHotkeyFormatInvalid
            | Self::SettingsHotkeyEmpty
            | Self::SettingsHotkeyConflict
            | Self::SettingsHotkeysIdentical
            | Self::SettingsRecordingFpsRange
            | Self::SettingsRecordingVideoBitrateRange
            | Self::SettingsRecordingAudioBitrateRange
            | Self::SettingsRecordingMaxDurationRange
            | Self::SettingsRecordingFileNameEmpty
            | Self::SettingsRecordingFfmpegUrlEmpty
            | Self::SettingsRecordingFfmpegUrlNotHttps
            | Self::SettingsRecordingAudioSyncRange
            | Self::SettingsClipboardBottomOffsetRange
            | Self::SettingsTranslationPromptEmpty
            | Self::SettingsExplanationPromptEmpty
            | Self::SettingsTranslationPromptMissingPlaceholder
            | Self::SettingsExplanationPromptMissingPlaceholder
            | Self::SettingsProviderNameEmpty
            | Self::SettingsSaveProviderFailed
            | Self::SettingsSaveFailed
            | Self::SettingsValidationFailed
            | Self::SettingsApiKeySaveFailed
            | Self::SettingsApiKeyGetFailed
            | Self::SettingsCredentialCreateFailed
            | Self::SettingsLocalKeyNotFound => ErrorCode::ValidationError,
            Self::AiNotConfigured
            | Self::AiProviderNotFound
            | Self::AiApiUrlEmpty
            | Self::AiModelNameEmpty
            | Self::AiApiUrlInvalid
            | Self::AiApiKeyNotConfigured
            | Self::AiKeychainReadFailed => ErrorCode::ConfigError,
            Self::AiClientInitFailed
            | Self::AiConnectionTestFailed
            | Self::AiConnectionTestNoResponse
            | Self::AiTextEmpty => ErrorCode::NetworkError,
            Self::ClipboardHotkeyRegisterFailed
            | Self::ClipboardCategoryAddFailed
            | Self::ClipboardCategoryRemoveFailed
            | Self::ClipboardCategorySetFailed
            | Self::ClipboardDeleteTextFailed
            | Self::ClipboardDeleteImageFailed
            | Self::ClipboardItemNotFound
            | Self::ClipboardPinFailed
            | Self::ClipboardPinImageFailed
            | Self::ClipboardWarmupFailed
            | Self::ClipboardPreviewPathFailed
            | Self::ClipboardPreviewShowFailed
            | Self::ClipboardImagePathEmpty
            | Self::ClipboardImageFileNotFound
            | Self::ClipboardImageFormatUnsupported
            | Self::ClipboardSetTagsFailed
            | Self::ClipboardUpdateContentFailed
            | Self::ClipboardSetPinFailed
            | Self::ClipboardCleanTextFailed
            | Self::ClipboardCleanImageFailed
            | Self::ClipboardNoFilesSelected
            | Self::ClipboardNoImagesFound
            | Self::ClipboardNoImagesImported
            | Self::ClipboardImportFailed
            | Self::ClipboardCopyTextFailed
            | Self::ClipboardAutoPasteFailed => ErrorCode::ClipboardError,
            Self::ScreenshotSourceFileNotFound
            | Self::ScreenshotTargetDirEmpty
            | Self::ScreenshotSavePathEmpty
            | Self::ScreenshotUnsupportedOperation
            | Self::ScreenshotFeatureDisabled
            | Self::ScreenshotFailed
            | Self::ScreenshotWriteSourceFailed
            | Self::ScreenshotCreateWindowFailed
            | Self::ScreenshotLongshotStatusFailed
            | Self::LongshotAreaTooSmall
            | Self::LongshotAlreadyRunning
            | Self::LongshotSessionNotFound
            | Self::LongshotSessionIdMismatch
            | Self::LongshotNoValidCapture
            | Self::LongshotNoSegments
            | Self::LongshotResultEmpty
            | Self::LongshotFrameTooSmall
            | Self::LongshotAreaTooLarge
            | Self::LongshotDependencyMissing
            | Self::LongshotFfmpegReadFailed
            | Self::LongshotCancelled => ErrorCode::SystemError,
            Self::RecordingFeatureDisabled
            | Self::RecordingFfmpegNotFound
            | Self::RecordingStartFailed
            | Self::RecordingStopFailed
            | Self::RecordingPauseFailed
            | Self::RecordingResumeFailed
            | Self::RecordingWindowInvalid
            | Self::RecordingWindowInvisible
            | Self::RecordingWindowMinimized => ErrorCode::SystemError,
            Self::BackupDirNotConfigured
            | Self::BackupDirNotSet
            | Self::BackupInvalidFile
            | Self::BackupDeleteOutsideDir => ErrorCode::ConfigError,
            Self::DocumentDirHasFiles
            | Self::DocumentCategoryHasFiles
            | Self::DocumentPathNotDir
            | Self::DocumentCategoryNameEmpty
            | Self::DocumentCategoryNameInvalidChar
            | Self::DocumentFileNotFound
            | Self::DocumentMoveFailed
            | Self::DocumentDeleteFailed
            | Self::DocumentRenameFailed
            | Self::DocumentScanFailed
            | Self::DocumentImportFailed
            | Self::DocumentDetectionFailed
            | Self::DocumentDirNotFound => ErrorCode::SystemError,
            Self::LauncherStartupFailed
            | Self::LauncherCommandPrefixExists
            | Self::LauncherCommandNotFound
            | Self::LauncherShortcutNotFound
            | Self::LauncherShortcutResolveFailed
            | Self::LauncherAppDirFailed
            | Self::LauncherNotWindows => ErrorCode::SystemError,
            Self::SelectionFeatureDisabled
            | Self::SelectionClipboardDisabled
            | Self::SelectionImageClipboardDisabled
            | Self::SelectionCtrlReleaseFailed
            | Self::SelectionWaitHideTimeout => ErrorCode::SystemError,
            Self::VcRuntimeDownloadUrlEmpty
            | Self::VcRuntimeDownloadUrlSha256Invalid => ErrorCode::ValidationError,
            Self::ImageStoreReadFailed
            | Self::ImageStoreWriteFailed
            | Self::ImageStoreInitFailed
            | Self::ImageStorePoolFailed
            | Self::ImageStoreBatchDeleteFailed
            | Self::DatabaseTargetNotFound
            | Self::DatabaseError => ErrorCode::SystemError,
            Self::IoError => ErrorCode::IoError,
            Self::JsonError => ErrorCode::SystemError,
            Self::SystemImageDataEmpty
            | Self::SystemLocalImageEmpty
            | Self::SystemIndexOutOfRange
            | Self::SystemUnsupportedCleanMode
            | Self::SystemPreviewGenerating
            | Self::SystemWebImageNoData
            | Self::SystemClipboardNotBitmap
            | Self::SystemWriteClipboardFailed => ErrorCode::SystemError,
        }
    }

    /// 机器可读键名（SCREAMING_SNAKE_CASE 格式）
    pub fn to_key(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "UNKNOWN".to_string())
    }

    /// 转换为 AppError
    pub fn to_app_error(&self) -> crate::core::error::AppError {
        crate::core::error::AppError::new(self.category(), self.default_message())
    }

    /// 转换为带详情的 AppError
    pub fn to_app_error_with_details(&self, details: impl Into<String>) -> crate::core::error::AppError {
        crate::core::error::AppError::new(self.category(), self.default_message()).with_details(details)
    }

    /// 直接转换为前端 JSON 错误字符串
    pub fn to_frontend_json(&self) -> String {
        crate::core::frontend_error::to_frontend_error_json(*self)
    }

    /// 转换为带详情的前端 JSON 错误字符串
    pub fn to_frontend_json_with_details(&self, details: impl Into<String>) -> String {
        let details = details.into();
        crate::core::frontend_error::to_frontend_error_json_with_details(*self, None, details)
    }

    /// 转换为带参数的前端 JSON 错误字符串
    pub fn to_frontend_json_with_params(&self, params: serde_json::Value) -> String {
        crate::core::frontend_error::to_frontend_error_json_with_params(*self, params)
    }
}
