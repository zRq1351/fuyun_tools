import {invoke as tauriInvoke} from '@tauri-apps/api/core';

/**
 * 带统一错误处理的 IPC 调用包装器
 * 将 Rust 后端的错误转换为包含上下文信息的 Error 对象
 * @param {string} command - IPC 命令名
 * @param {object} [args] - 命令参数
 * @returns {Promise<any>}
 */
async function ipcInvoke(command, args) {
    try {
        return await tauriInvoke(command, args);
    } catch (error) {
        const msg = typeof error === 'string' ? error : error?.message || String(error);
        const err = new Error(`IPC ${command} failed: ${msg}`);
        err.originalError = error;
        err.command = command;
        throw err;
    }
}

const buildSelectAndFillRequest = (itemId, opId) => ({
    itemId,
    opId
});

const buildStreamTranslateRequest = (text, sourceLanguage, targetLanguage, opId, sceneHint, windowLabel) => ({
    text,
    sourceLanguage,
    targetLanguage,
    opId,
    sceneHint,
    windowLabel: windowLabel || null
});
const buildStreamExplainRequest = (text, targetLanguage, opId, sceneHint, windowLabel) => ({
    text,
    targetLanguage,
    opId,
    sceneHint,
    windowLabel: windowLabel || null
});

/**
 * IPC 命令常量定义
 * @enum {string}
 */
export const IPC_COMMANDS = {
    // 剪贴板管理
    GET_CLIPBOARD_HISTORY: 'get_clipboard_history',
    GET_CLIPBOARD_HISTORY_PAGE: 'get_clipboard_history_page',
    REMOVE_CLIPBOARD_ITEM: 'remove_clipboard_item',
    SET_CLIPBOARD_ITEM_PINNED: 'set_clipboard_item_pinned',
    PROMOTE_CLIPBOARD_ITEM: 'promote_clipboard_item',
    CLEAR_TEXT_HISTORY: 'clear_text_history',
    SELECT_AND_FILL: 'select_and_fill',
    GET_IMAGE_CLIPBOARD_HISTORY: 'get_image_clipboard_history',
    GET_IMAGE_CLIPBOARD_HISTORY_PAGE: 'get_image_clipboard_history_page',
    REMOVE_IMAGE_CLIPBOARD_ITEM_BY_ID: 'remove_image_clipboard_item_by_id',
    PROMOTE_IMAGE_CLIPBOARD_ITEM_BY_ID: 'promote_image_clipboard_item_by_id',
    CLEAR_IMAGE_HISTORY: 'clear_image_history',
    IMPORT_IMAGE_FILES: 'import_image_files',
    COUNT_IMPORT_IMAGE_FILES: 'count_import_image_files',
    SELECT_AND_FILL_IMAGE_BY_ID: 'select_and_fill_image_by_id',
    WARMUP_IMAGE_CLIPBOARD_ITEM_BY_ID: 'warmup_image_clipboard_item_by_id',
    WARMUP_MULTIPLE_IMAGES: 'warmup_multiple_images',
    OPEN_IMAGE_PREVIEW_WINDOW_BY_ID: 'open_image_preview_window_by_id',
    CLOSE_IMAGE_PREVIEW_WINDOW: 'close_image_preview_window',
    START_IMAGE_PREVIEW_WINDOW_DRAG: 'start_image_preview_window_drag',
    OPEN_TEXT_PREVIEW_WINDOW: 'open_text_preview_window',
    CLOSE_TEXT_PREVIEW_WINDOW: 'close_text_preview_window',
    START_TEXT_PREVIEW_WINDOW_DRAG: 'start_text_preview_window_drag',
    COPY_IMAGE_CLIPBOARD_ITEM_TO_DIRECTORY: 'copy_image_clipboard_item_to_directory',
    COPY_TEXT: 'copy_text',
    COPY_AND_PASTE_TEXT: 'copy_and_paste_text',
    GET_CLIPBOARD_FULL_SNAPSHOT: 'get_clipboard_full_snapshot',
    UPDATE_TEXT_ITEM: 'update_text_item',

    // 异步预览
    GET_IMAGE_PREVIEW_BY_ID: 'get_image_preview_by_id',
    CHECK_PREVIEWS_READY: 'check_previews_ready',

    // 分类管理
    SET_ITEM_CATEGORY: 'set_item_category',
    REMOVE_CATEGORY: 'remove_category',
    ADD_CATEGORY: 'add_category',
    SET_IMAGE_ITEM_CATEGORY: 'set_image_item_category',
    SET_IMAGE_ITEM_TAGS: 'set_image_item_tags',
    SET_IMAGE_ITEM_PINNED: 'set_image_item_pinned',
    REMOVE_IMAGE_CATEGORY: 'remove_image_category',
    ADD_IMAGE_CATEGORY: 'add_image_category',

    // 窗口管理
    GET_CLIPBOARD_BOTTOM_OFFSET: 'get_clipboard_bottom_offset',
    PREVIEW_CLIPBOARD_BOTTOM_OFFSET: 'preview_clipboard_bottom_offset',
    SAVE_CLIPBOARD_BOTTOM_OFFSET: 'save_clipboard_bottom_offset',
    WINDOW_BLUR: 'window_blur',
    IMAGE_WINDOW_BLUR: 'image_window_blur',
    SELECTION_TOOLBAR_BLUR: 'selection_toolbar_blur',
    OPEN_SETTINGS_WINDOW: 'open_settings_window',
    RESIZE_SELECTION_TOOLBAR: 'resize_selection_toolbar',

    // AI 设置
    GET_AI_SETTINGS: 'get_ai_settings',
    CHECK_VC_RUNTIME_DEPENDENCIES: 'check_vc_runtime_dependencies',
    DOWNLOAD_VC_RUNTIME_INSTALLER: 'download_vc_runtime_installer',
    OPEN_VC_RUNTIME_INSTALLER: 'open_vc_runtime_installer',
    INSTALL_VC_RUNTIME_AND_WAIT: 'install_vc_runtime_and_wait',
    SAVE_APP_SETTINGS: 'save_app_settings',
    TEST_AI_CONNECTION: 'test_ai_connection',
    REMOVE_AI_PROVIDER: 'remove_ai_provider',
    GET_ALL_CONFIGURED_PROVIDERS: 'get_all_configured_providers',
    START_RECORDING: 'start_recording',
    PAUSE_RECORDING: 'pause_recording',
    RESUME_RECORDING: 'resume_recording',
    UPDATE_RECORDING_AUDIO_CAPTURE: 'update_recording_audio_capture',
    STOP_RECORDING: 'stop_recording',
    CANCEL_RECORDING: 'cancel_recording',
    GET_RECORDING_STATE: 'get_recording_state',
    GET_RECORDING_OUTPUT_DIR: 'get_recording_output_dir',
    GET_WINDOW_LIST: 'get_window_list',
    START_MANUAL_LONGSHOT: 'start_manual_longshot',
    PAUSE_MANUAL_LONGSHOT: 'pause_manual_longshot',
    RESUME_MANUAL_LONGSHOT: 'resume_manual_longshot',
    CANCEL_MANUAL_LONGSHOT: 'cancel_manual_longshot',
    FINISH_MANUAL_LONGSHOT: 'finish_manual_longshot',
    GET_MANUAL_LONGSHOT_STATUS: 'get_manual_longshot_status',
    GET_MANUAL_LONGSHOT_AVAILABILITY: 'get_manual_longshot_availability',
    PREVIEW_BACKUP_EXPORT: 'preview_backup_export',
    EXPORT_BACKUP_TO_PATH: 'export_backup_to_path',
    PREVIEW_BACKUP_PACKAGE: 'preview_backup_package',
    RESTORE_BACKUP_PACKAGE: 'restore_backup_package',
    LIST_BACKUP_HISTORY: 'list_backup_history',
    DELETE_BACKUP_HISTORY_ITEM: 'delete_backup_history_item',
    RUN_MANUAL_BACKUP: 'run_manual_backup',
    GET_BACKUP_SETTINGS: 'get_backup_settings',
    SAVE_BACKUP_SETTINGS: 'save_backup_settings',
    GET_DIAGNOSTIC_OVERVIEW: 'get_diagnostic_overview',
    GET_DIAGNOSTIC_ITEMS: 'get_diagnostic_items',
    RUN_DIAGNOSTIC_ACTION: 'run_diagnostic_action',
    LIST_RECORDING_AUDIO_DEVICES: 'list_recording_audio_devices',
    LIST_RECORDING_SYSTEM_OUTPUT_DEVICES: 'list_recording_system_output_devices',
    LIST_RECORDING_AUDIO_PROCESSES: 'list_recording_audio_processes',
    LIST_RECORDING_MONITORS: 'list_recording_monitors',
    OPEN_RECORDING_FOLDER: 'open_recording_folder',
    RUN_RECORDING_REGRESSION: 'run_recording_regression',
    RESIZE_RECORDING_TOOLBAR: 'resize_recording_toolbar',
    CHECK_RECORDING_FFMPEG: 'check_recording_ffmpeg',
    DOWNLOAD_RECORDING_FFMPEG: 'download_recording_ffmpeg',

    // 文档管理
    ADD_DOC_ROOT: 'add_doc_root',
    GET_DOC_ROOTS: 'get_doc_roots',
    REMOVE_DOC_ROOT: 'remove_doc_root',
    ADD_DOC_CATEGORY: 'add_doc_category',
    GET_DOC_CATEGORIES: 'get_doc_categories',
    REMOVE_DOC_CATEGORY: 'remove_doc_category',
    RENAME_DOC_CATEGORY: 'rename_doc_category',
    REORDER_DOC_CATEGORIES: 'reorder_doc_categories',
    REORDER_DOC_ROOTS: 'reorder_doc_roots',
    REORDER_DOC_FILES: 'reorder_doc_files',
    IMPORT_FILES: 'import_files',
    GET_DOC_PAGE: 'get_doc_page',
    UPDATE_DOC_META: 'update_doc_meta',
    DELETE_DOC: 'delete_doc',
    MOVE_DOC: 'move_doc',
    ATOMIC_MOVE_DOC: 'atomic_move_doc',
    GET_DOC_STATS: 'get_doc_stats',
    OPEN_DOC: 'open_doc',
    OPEN_DOC_FOLDER: 'open_doc_folder',
    GET_DOC_DETAIL: 'get_doc_detail',
    SCAN_FOLDER: 'scan_folder',
    GET_IMPORT_HISTORY: 'get_import_history',
    UNDO_IMPORT: 'undo_import',
    UNDO_IMPORT_ITEM: 'undo_import_item',
    GET_IMPORT_FILES: 'get_import_files',
    DETECT_ORPHAN_FILES: 'detect_orphan_files',
    GET_FILE_TYPE_ICON: 'get_file_type_icon',
    GET_FILE_TYPE_ICONS: 'get_file_type_icons',

    // AI 功能
    STREAM_TRANSLATE_TEXT: 'stream_translate_text',
    STREAM_EXPLAIN_TEXT: 'stream_explain_text',
    STREAM_CUSTOM_PROMPT_TEXT: 'stream_custom_prompt_text',
    ...(__DEV_PANEL__ ? {
        GET_TEXT_DEDUP_METRICS: 'get_text_dedup_metrics',
        GET_IMAGE_STORAGE_METRICS: 'get_image_storage_metrics',
        GET_IMAGE_PERSIST_QUEUE_METRICS: 'get_image_persist_queue_metrics',
        GET_COPY_PASTE_DEDUP_DEBUG_STATE: 'get_copy_paste_dedup_debug_state',
        SET_COPY_PASTE_DEDUP_DEBUG_CONFIG: 'set_copy_paste_dedup_debug_config',
        GET_VC_RUNTIME_DEBUG_STATE: 'get_vc_runtime_debug_state',
        SET_VC_RUNTIME_DEBUG_CONFIG: 'set_vc_runtime_debug_config',
    } : {}),
};

/**
 * 剪贴板相关的 IPC 服务
 */
export const ClipboardService = {
    /**
     * 获取剪贴板历史记录
     * @returns {Promise<{history: string[], categories: Object, category_list: string[]}>}
     */
    getHistory: () => ipcInvoke(IPC_COMMANDS.GET_CLIPBOARD_HISTORY),

    /**
     * 批量获取剪贴板完整快照（优化 IPC 通信）
     * 一次 IPC 调用获取所有需要的数据，减少通信开销
     * @returns {Promise<{
     *   textHistory: string[],
     *   textCategories: Object,
     *   textCategoryList: string[],
     *   textPinnedItems: string[],
     *   imageHistory: Array,
     *   imageCategories: Object,
     *   imageCategoryList: string[],
     *   imageTags: Object,
     *   imagePinnedItems: string[]
     * }>}
     */
    getFullSnapshot: () => ipcInvoke(IPC_COMMANDS.GET_CLIPBOARD_FULL_SNAPSHOT),
    getHistoryPage: ({
                         offset = 0,
                         limit = 50,
                         category = null,
                         pinnedOnly = false,
                         keyword = null,
                         sortBy = null,
                         sortOrder = null
                     } = {}) =>
        ipcInvoke(IPC_COMMANDS.GET_CLIPBOARD_HISTORY_PAGE, {
            request: {offset, limit, category, pinnedOnly, keyword, sortBy, sortOrder}
        }),

    /**
     * 删除剪贴板项目
     * @param {string} itemId
     * @returns {Promise<void>}
     */
    removeItem: (itemId) => ipcInvoke(IPC_COMMANDS.REMOVE_CLIPBOARD_ITEM, { itemId }),
    setItemPinned: (itemId, pinned) => ipcInvoke(IPC_COMMANDS.SET_CLIPBOARD_ITEM_PINNED, { itemId, pinned }),
    promoteItem: (itemId) => ipcInvoke(IPC_COMMANDS.PROMOTE_CLIPBOARD_ITEM, { itemId }),
    clearHistory: (mode) => ipcInvoke(IPC_COMMANDS.CLEAR_TEXT_HISTORY, {mode}),

    /**
     * 选择并填充内容
     * @param {string} itemId
     * @returns {Promise<void>}
     */
    selectAndFill: (itemId, opId) =>
        ipcInvoke(IPC_COMMANDS.SELECT_AND_FILL, {request: buildSelectAndFillRequest(itemId, opId)}),

    /**
     * 复制文本到剪贴板
     * @param {string} text
     * @returns {Promise<void>}
     */
    copyText: (text) => ipcInvoke(IPC_COMMANDS.COPY_TEXT, {text}),
    copyAndPasteText: (text, requestId = null) => ipcInvoke(IPC_COMMANDS.COPY_AND_PASTE_TEXT, {text, requestId}),
};

export const ImageClipboardService = {
    getHistory: () => ipcInvoke(IPC_COMMANDS.GET_IMAGE_CLIPBOARD_HISTORY),
    getHistoryPage: ({
                         offset = 0,
                         limit = 50,
                         category = null,
                         keyword = null,
                         pinnedOnly = false,
                         sortBy = 'pinnedFirst',
                         sortOrder = null
                     } = {}) =>
        ipcInvoke(IPC_COMMANDS.GET_IMAGE_CLIPBOARD_HISTORY_PAGE, {
            request: {offset, limit, category, keyword, pinnedOnly, sortBy, sortOrder}
        }),
    removeItemById: (itemId) => ipcInvoke(IPC_COMMANDS.REMOVE_IMAGE_CLIPBOARD_ITEM_BY_ID, {itemId}),
    promoteItemById: (itemId) =>
        ipcInvoke(IPC_COMMANDS.PROMOTE_IMAGE_CLIPBOARD_ITEM_BY_ID, {request: {itemId}}),
    setItemPinned: (itemId, pinned) => ipcInvoke(IPC_COMMANDS.SET_IMAGE_ITEM_PINNED, {itemId, pinned}),
    clearHistory: (mode) => ipcInvoke(IPC_COMMANDS.CLEAR_IMAGE_HISTORY, {mode}),
    importImageFiles: (paths) => ipcInvoke(IPC_COMMANDS.IMPORT_IMAGE_FILES, {paths}),
    countImportImageFiles: (paths) => ipcInvoke(IPC_COMMANDS.COUNT_IMPORT_IMAGE_FILES, {paths}),
    selectAndFillById: (itemId, opId) =>
        ipcInvoke(IPC_COMMANDS.SELECT_AND_FILL_IMAGE_BY_ID, {
            request: buildSelectAndFillRequest(itemId, opId)
        }),
    warmupItemById: (itemId) =>
        ipcInvoke(IPC_COMMANDS.WARMUP_IMAGE_CLIPBOARD_ITEM_BY_ID, {request: {itemId}}),
    warmupMultipleItems: (itemIds) =>
        ipcInvoke(IPC_COMMANDS.WARMUP_MULTIPLE_IMAGES, {itemIds}),
    openPreviewWindowById: (itemId) =>
        ipcInvoke(IPC_COMMANDS.OPEN_IMAGE_PREVIEW_WINDOW_BY_ID, {request: {itemId}}),
    closePreviewWindow: () => ipcInvoke(IPC_COMMANDS.CLOSE_IMAGE_PREVIEW_WINDOW),
    startPreviewWindowDrag: () => ipcInvoke(IPC_COMMANDS.START_IMAGE_PREVIEW_WINDOW_DRAG),

    openTextPreviewWindow: (text, itemId = null) =>
        ipcInvoke(IPC_COMMANDS.OPEN_TEXT_PREVIEW_WINDOW, {text, itemId}),
    closeTextPreviewWindow: () => ipcInvoke(IPC_COMMANDS.CLOSE_TEXT_PREVIEW_WINDOW),
    startTextPreviewWindowDrag: () => ipcInvoke(IPC_COMMANDS.START_TEXT_PREVIEW_WINDOW_DRAG),
    updateTextItem: (itemId, newContent) =>
        ipcInvoke(IPC_COMMANDS.UPDATE_TEXT_ITEM, {itemId, newContent}),

    copyItemToDirectory: (itemId, targetDirectory) =>
        ipcInvoke(IPC_COMMANDS.COPY_IMAGE_CLIPBOARD_ITEM_TO_DIRECTORY, {itemId, targetDirectory}),
    setItemTags: (itemId, tags) => ipcInvoke(IPC_COMMANDS.SET_IMAGE_ITEM_TAGS, {itemId, tags}),

    // 异步预览相关方法
    getImagePreviewById: (itemId) =>
        ipcInvoke(IPC_COMMANDS.GET_IMAGE_PREVIEW_BY_ID, {itemId}),
    checkPreviewsReady: (itemIds) =>
        ipcInvoke(IPC_COMMANDS.CHECK_PREVIEWS_READY, {itemIds}),
};

/**
 * 分类管理相关的 IPC 服务
 */
export const CategoryService = {
    /**
     * 设置条目分类
     * @param {string} itemId
     * @param {string} category
     * @returns {Promise<void>}
     */
    setItemCategory: (itemId, category) => ipcInvoke(IPC_COMMANDS.SET_ITEM_CATEGORY, {itemId, category}),

    /**
     * 删除分类
     * @param {string} category
     * @returns {Promise<void>}
     */
    removeCategory: (category) => ipcInvoke(IPC_COMMANDS.REMOVE_CATEGORY, {category}),

    /**
     * 添加分类
     * @param {string} category
     * @returns {Promise<void>}
     */
    addCategory: (category) => ipcInvoke(IPC_COMMANDS.ADD_CATEGORY, {category}),
};

export const ImageCategoryService = {
    setItemCategory: (itemId, category) => ipcInvoke(IPC_COMMANDS.SET_IMAGE_ITEM_CATEGORY, {itemId, category}),
    removeCategory: (category) => ipcInvoke(IPC_COMMANDS.REMOVE_IMAGE_CATEGORY, {category}),
    addCategory: (category) => ipcInvoke(IPC_COMMANDS.ADD_IMAGE_CATEGORY, {category}),
};

/**
 * 窗口管理相关的 IPC 服务
 */
export const WindowService = {
    /**
     * 获取窗口底部偏移量
     * @returns {Promise<number>}
     */
    getBottomOffset: () => ipcInvoke(IPC_COMMANDS.GET_CLIPBOARD_BOTTOM_OFFSET),

    /**
     * 预览窗口底部偏移量
     * @param {number} offset
     * @returns {Promise<void>}
     */
    previewBottomOffset: (offset) => ipcInvoke(IPC_COMMANDS.PREVIEW_CLIPBOARD_BOTTOM_OFFSET, {offset}),

    /**
     * 保存窗口底部偏移量
     * @param {number} offset
     * @returns {Promise<void>}
     */
    saveBottomOffset: (offset) => ipcInvoke(IPC_COMMANDS.SAVE_CLIPBOARD_BOTTOM_OFFSET, {offset}),

    /**
     * 窗口失去焦点通知
     * @returns {Promise<void>}
     */
    blur: () => ipcInvoke(IPC_COMMANDS.WINDOW_BLUR),
    imageBlur: () => ipcInvoke(IPC_COMMANDS.IMAGE_WINDOW_BLUR),
    openSettingsWindow: (tab = 'ai', reason = '') => ipcInvoke(IPC_COMMANDS.OPEN_SETTINGS_WINDOW, {tab, reason}),

    /**
     * 选择工具栏失去焦点通知
     * @returns {Promise<void>}
     */
    selectionToolbarBlur: () => ipcInvoke(IPC_COMMANDS.SELECTION_TOOLBAR_BLUR),
    resizeSelectionToolbar: (x, y, width, height) => ipcInvoke(IPC_COMMANDS.RESIZE_SELECTION_TOOLBAR, {x, y, width, height}),
};

/**
 * AI 设置相关的 IPC 服务
 */
export const AISettingsService = {
    /**
     * 获取 AI 设置
     * @returns {Promise<Object>}
     */
    getSettings: () => ipcInvoke(IPC_COMMANDS.GET_AI_SETTINGS),
    checkVcRuntimeDependencies: () => ipcInvoke(IPC_COMMANDS.CHECK_VC_RUNTIME_DEPENDENCIES),
    downloadVcRuntimeInstaller: (downloadUrl = null) =>
        ipcInvoke(IPC_COMMANDS.DOWNLOAD_VC_RUNTIME_INSTALLER, {downloadUrl}),
    openVcRuntimeInstaller: (installerPath) =>
        ipcInvoke(IPC_COMMANDS.OPEN_VC_RUNTIME_INSTALLER, {installerPath}),
    installVcRuntimeAndWait: (installerPath) =>
        ipcInvoke(IPC_COMMANDS.INSTALL_VC_RUNTIME_AND_WAIT, {installerPath}),

    /**
     * 保存应用设置（传入变化的字段对象即可）
     * @param {Object} settings - 要保存的设置字段
     * @returns {Promise<void>}
     */
    saveSettings: (settings) =>
        ipcInvoke(IPC_COMMANDS.SAVE_APP_SETTINGS, settings),

    /**
     * 测试 AI 连接
     * @param {Object} params
     * @param {string} params.aiProvider
     * @param {string} params.aiApiUrl
     * @param {string} params.aiModelName
     * @param {string} params.aiApiKey
     * @returns {Promise<string>}
     */
    testConnection: ({aiProvider, aiApiUrl, aiModelName, aiApiKey}) =>
        ipcInvoke(IPC_COMMANDS.TEST_AI_CONNECTION, {aiProvider, aiApiUrl, aiModelName, aiApiKey}),

    /**
     * 获取提供商配置
     * @param {string} provider
     * @returns {Promise<[string, string]>} [url, model]
     */

    /**
     * 删除 AI 提供商
     * @param {string} provider
     * @returns {Promise<void>}
     */
    removeProvider: (provider) => ipcInvoke(IPC_COMMANDS.REMOVE_AI_PROVIDER, {provider}),

    /**
     * 获取所有已配置的提供商
     * @returns {Promise<Array<[string, string]>>}
     */
    getAllConfiguredProviders: () => ipcInvoke(IPC_COMMANDS.GET_ALL_CONFIGURED_PROVIDERS),
    ...(__DEV_PANEL__ ? {
        getTextDedupMetrics: () =>
            ipcInvoke(IPC_COMMANDS.GET_TEXT_DEDUP_METRICS),
        getImageStorageMetrics: () =>
            ipcInvoke(IPC_COMMANDS.GET_IMAGE_STORAGE_METRICS),
        getImagePersistQueueMetrics: () =>
            ipcInvoke(IPC_COMMANDS.GET_IMAGE_PERSIST_QUEUE_METRICS),
        getCopyPasteDedupDebugState: () =>
            ipcInvoke(IPC_COMMANDS.GET_COPY_PASTE_DEDUP_DEBUG_STATE),
        setCopyPasteDedupDebugConfig: ({enabled, windowMs, logEnabled, resetMetrics}) =>
            ipcInvoke(IPC_COMMANDS.SET_COPY_PASTE_DEDUP_DEBUG_CONFIG, {enabled, windowMs, logEnabled, resetMetrics}),
        getVcRuntimeDebugState: () =>
            ipcInvoke(IPC_COMMANDS.GET_VC_RUNTIME_DEBUG_STATE),
        setVcRuntimeDebugConfig: ({forceMissing}) =>
            ipcInvoke(IPC_COMMANDS.SET_VC_RUNTIME_DEBUG_CONFIG, {forceMissing}),
    } : {}),
};

/**
 * AI 功能相关的 IPC 服务
 */
export const AIService = {
    /**
     * 流式翻译文本
     * @param {string} text
     * @param {string} sourceLanguage
     * @param {string} targetLanguage
     * @returns {Promise<void>}
     */
    streamTranslate: (text, sourceLanguage, targetLanguage, opId, sceneHint, windowLabel) =>
        ipcInvoke(IPC_COMMANDS.STREAM_TRANSLATE_TEXT, {
            request: buildStreamTranslateRequest(text, sourceLanguage, targetLanguage, opId, sceneHint, windowLabel)
        }),

    /**
     * 流式解释文本
     * @param {string} text
     * @param {string} targetLanguage
     * @returns {Promise<void>}
     */
    streamExplain: (text, targetLanguage, opId, sceneHint, windowLabel) =>
        ipcInvoke(IPC_COMMANDS.STREAM_EXPLAIN_TEXT, {
            request: buildStreamExplainRequest(text, targetLanguage, opId, sceneHint, windowLabel)
        }),

    /**
     * 流式执行自定义 Prompt
     * @param {string} text 选中文本
     * @param {string} promptName Prompt 名称
     * @returns {Promise<void>}
     */
    streamCustomPrompt: (text, promptName, targetLanguage, opId, sceneHint) =>
        ipcInvoke(IPC_COMMANDS.STREAM_CUSTOM_PROMPT_TEXT, {
            request: {text, promptName, targetLanguage, opId, sceneHint}
        }),
};

export const RecordingService = {
    start: (request = {}) => ipcInvoke(IPC_COMMANDS.START_RECORDING, {request}),
    pause: () => ipcInvoke(IPC_COMMANDS.PAUSE_RECORDING),
    resume: () => ipcInvoke(IPC_COMMANDS.RESUME_RECORDING),
    updateAudioCapture: (request = {}) => ipcInvoke(IPC_COMMANDS.UPDATE_RECORDING_AUDIO_CAPTURE, {request}),
    stop: (sessionId = null) => ipcInvoke(IPC_COMMANDS.STOP_RECORDING, {request: {sessionId}}),
    cancel: (sessionId = null) => ipcInvoke(IPC_COMMANDS.CANCEL_RECORDING, {request: {sessionId}}),
    getState: () => ipcInvoke(IPC_COMMANDS.GET_RECORDING_STATE),
    getOutputDir: () => ipcInvoke(IPC_COMMANDS.GET_RECORDING_OUTPUT_DIR),
    listWindows: () => ipcInvoke(IPC_COMMANDS.GET_WINDOW_LIST),
    listAudioDevices: () => ipcInvoke(IPC_COMMANDS.LIST_RECORDING_AUDIO_DEVICES),
    listSystemOutputs: () => ipcInvoke(IPC_COMMANDS.LIST_RECORDING_SYSTEM_OUTPUT_DEVICES),
    listAudioProcesses: () => ipcInvoke(IPC_COMMANDS.LIST_RECORDING_AUDIO_PROCESSES),
    listMonitors: () => ipcInvoke(IPC_COMMANDS.LIST_RECORDING_MONITORS),
    openFolder: () => ipcInvoke(IPC_COMMANDS.OPEN_RECORDING_FOLDER),
    runRegression: () => ipcInvoke(IPC_COMMANDS.RUN_RECORDING_REGRESSION),
    checkFfmpeg: () => ipcInvoke(IPC_COMMANDS.CHECK_RECORDING_FFMPEG),
    downloadFfmpeg: (downloadUrl = null) => ipcInvoke(IPC_COMMANDS.DOWNLOAD_RECORDING_FFMPEG, {downloadUrl}),
    resizeToolbar: (
        openSelect,
        openOverlay,
        compactMode = false,
        layoutMode = 'capsule',
        recenter = false,
        capsuleContentHeight = null,
        capsuleContentWidth = null,
        keepWidth = false
    ) => ipcInvoke(IPC_COMMANDS.RESIZE_RECORDING_TOOLBAR, {
        request: {
            openSelect,
            openOverlay,
            compactMode,
            layoutMode,
            recenter,
            capsuleContentHeight,
            capsuleContentWidth,
            keepWidth
        }
    }),
};

export const ScreenshotService = {
    startManualLongshot: (request) => ipcInvoke(IPC_COMMANDS.START_MANUAL_LONGSHOT, {request}),
    pauseManualLongshot: (sessionId) => ipcInvoke(IPC_COMMANDS.PAUSE_MANUAL_LONGSHOT, {request: {sessionId}}),
    resumeManualLongshot: (sessionId) => ipcInvoke(IPC_COMMANDS.RESUME_MANUAL_LONGSHOT, {request: {sessionId}}),
    cancelManualLongshot: (sessionId) => ipcInvoke(IPC_COMMANDS.CANCEL_MANUAL_LONGSHOT, {request: {sessionId}}),
    finishManualLongshot: (sessionId) => ipcInvoke(IPC_COMMANDS.FINISH_MANUAL_LONGSHOT, {request: {sessionId}}),
    getManualLongshotStatus: (sessionId) => ipcInvoke(IPC_COMMANDS.GET_MANUAL_LONGSHOT_STATUS, {request: {sessionId}}),
    getManualLongshotAvailability: () => ipcInvoke(IPC_COMMANDS.GET_MANUAL_LONGSHOT_AVAILABILITY),
};

export const BackupService = {
    previewExport: () => ipcInvoke(IPC_COMMANDS.PREVIEW_BACKUP_EXPORT),
    exportToPath: (targetPath) => ipcInvoke(IPC_COMMANDS.EXPORT_BACKUP_TO_PATH, {request: {targetPath}}),
    previewPackage: (packagePath) => ipcInvoke(IPC_COMMANDS.PREVIEW_BACKUP_PACKAGE, {request: {packagePath}}),
    restorePackage: (payload) => ipcInvoke(IPC_COMMANDS.RESTORE_BACKUP_PACKAGE, {request: payload}),
    listHistory: () => ipcInvoke(IPC_COMMANDS.LIST_BACKUP_HISTORY),
    deleteHistoryItem: (filePath) => ipcInvoke(IPC_COMMANDS.DELETE_BACKUP_HISTORY_ITEM, {request: {filePath}}),
    runManualBackup: () => ipcInvoke(IPC_COMMANDS.RUN_MANUAL_BACKUP),
    getSettings: () => ipcInvoke(IPC_COMMANDS.GET_BACKUP_SETTINGS),
    saveSettings: (payload) => ipcInvoke(IPC_COMMANDS.SAVE_BACKUP_SETTINGS, {request: payload}),
};

export const DiagnosticService = {
    getOverview: () => ipcInvoke(IPC_COMMANDS.GET_DIAGNOSTIC_OVERVIEW),
    getItems: () => ipcInvoke(IPC_COMMANDS.GET_DIAGNOSTIC_ITEMS),
    runAction: (actionKey) => ipcInvoke(IPC_COMMANDS.RUN_DIAGNOSTIC_ACTION, {request: {actionKey}}),
};

export const DocumentService = {
    addRoot: (name, rootPath) => ipcInvoke(IPC_COMMANDS.ADD_DOC_ROOT, {name, rootPath}),
    getRoots: () => ipcInvoke(IPC_COMMANDS.GET_DOC_ROOTS),
    removeRoot: (id) => ipcInvoke(IPC_COMMANDS.REMOVE_DOC_ROOT, {id}),

    addCategory: (name, icon, color, rootId) => ipcInvoke(IPC_COMMANDS.ADD_DOC_CATEGORY, {name, icon, color, rootId}),
    getCategories: (rootId) => ipcInvoke(IPC_COMMANDS.GET_DOC_CATEGORIES, {rootId}),
    removeCategory: (id) => ipcInvoke(IPC_COMMANDS.REMOVE_DOC_CATEGORY, {id}),
    renameCategory: (id, name) => ipcInvoke(IPC_COMMANDS.RENAME_DOC_CATEGORY, {id, name}),
    reorderCategories: (ids) => ipcInvoke(IPC_COMMANDS.REORDER_DOC_CATEGORIES, {ids}),
    reorderRoots: (ids) => ipcInvoke(IPC_COMMANDS.REORDER_DOC_ROOTS, {ids}),
    reorderFiles: (ids) => ipcInvoke(IPC_COMMANDS.REORDER_DOC_FILES, {ids}),

    importFiles: (request) => ipcInvoke(IPC_COMMANDS.IMPORT_FILES, {request}),
    getPage: (request) => ipcInvoke(IPC_COMMANDS.GET_DOC_PAGE, {request}),
    updateMeta: (request) => ipcInvoke(IPC_COMMANDS.UPDATE_DOC_META, {request}),
    deleteDoc: (id, deleteFile) => ipcInvoke(IPC_COMMANDS.DELETE_DOC, {request: {id, deleteFile}}),
    moveDoc: (id, newRootId) => ipcInvoke(IPC_COMMANDS.MOVE_DOC, {id, newRootId}),
    atomicMoveDoc: (request) => ipcInvoke(IPC_COMMANDS.ATOMIC_MOVE_DOC, {request}),
    getStats: (rootId) => ipcInvoke(IPC_COMMANDS.GET_DOC_STATS, {rootId}),
    openDoc: (id) => ipcInvoke(IPC_COMMANDS.OPEN_DOC, {id}),
    openFolder: (id) => ipcInvoke(IPC_COMMANDS.OPEN_DOC_FOLDER, {id}),
    getDetail: (id) => ipcInvoke(IPC_COMMANDS.GET_DOC_DETAIL, {id}),
    scanFolder: (path, recursive) => ipcInvoke(IPC_COMMANDS.SCAN_FOLDER, {path, recursive}),

    getImportHistory: (limit) => ipcInvoke(IPC_COMMANDS.GET_IMPORT_HISTORY, {limit}),
    undoImport: (importId) => ipcInvoke(IPC_COMMANDS.UNDO_IMPORT, {importId}),
    undoImportItem: (importId, docFileId) => ipcInvoke(IPC_COMMANDS.UNDO_IMPORT_ITEM, {importId, docFileId}),
    getImportFiles: (importId) => ipcInvoke(IPC_COMMANDS.GET_IMPORT_FILES, {importId}),
    detectOrphanFiles: (rootId) => ipcInvoke(IPC_COMMANDS.DETECT_ORPHAN_FILES, {rootId}),
    getFileTypeIcon: (ext) => ipcInvoke(IPC_COMMANDS.GET_FILE_TYPE_ICON, {ext}),
    getFileTypeIcons: (exts) => ipcInvoke(IPC_COMMANDS.GET_FILE_TYPE_ICONS, {exts}),
};
