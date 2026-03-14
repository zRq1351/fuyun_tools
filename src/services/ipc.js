import {invoke} from '@tauri-apps/api/core';

const buildSelectAndFillRequest = (index, opId) => ({index, opId});
const buildSelectAndFillImageRequest = (index, opId) => ({index, opId});
const buildStreamTranslateRequest = (text, sourceLanguage, targetLanguage, opId, sceneHint) => ({
    text,
    sourceLanguage,
    targetLanguage,
    opId,
    sceneHint
});
const buildStreamExplainRequest = (text, targetLanguage, opId, sceneHint) => ({
    text,
    targetLanguage,
    opId,
    sceneHint
});

/**
 * IPC 命令常量定义
 * @enum {string}
 */
export const IPC_COMMANDS = {
    // 剪贴板管理
    GET_CLIPBOARD_HISTORY: 'get_clipboard_history',
    REMOVE_CLIPBOARD_ITEM: 'remove_clipboard_item',
    SELECT_AND_FILL: 'select_and_fill',
    GET_IMAGE_CLIPBOARD_HISTORY: 'get_image_clipboard_history',
    REMOVE_IMAGE_CLIPBOARD_ITEM: 'remove_image_clipboard_item',
    SELECT_AND_FILL_IMAGE: 'select_and_fill_image',
    WARMUP_IMAGE_CLIPBOARD_ITEM: 'warmup_image_clipboard_item',
    OPEN_IMAGE_PREVIEW_WINDOW: 'open_image_preview_window',
    CLOSE_IMAGE_PREVIEW_WINDOW: 'close_image_preview_window',
    COPY_TEXT: 'copy_text',
    COPY_AND_PASTE_TEXT: 'copy_and_paste_text',

    // 分类管理
    SET_ITEM_CATEGORY: 'set_item_category',
    REMOVE_CATEGORY: 'remove_category',
    ADD_CATEGORY: 'add_category',
    SET_IMAGE_ITEM_CATEGORY: 'set_image_item_category',
    REMOVE_IMAGE_CATEGORY: 'remove_image_category',
    ADD_IMAGE_CATEGORY: 'add_image_category',

    // 窗口管理
    GET_CLIPBOARD_BOTTOM_OFFSET: 'get_clipboard_bottom_offset',
    PREVIEW_CLIPBOARD_BOTTOM_OFFSET: 'preview_clipboard_bottom_offset',
    SAVE_CLIPBOARD_BOTTOM_OFFSET: 'save_clipboard_bottom_offset',
    WINDOW_BLUR: 'window_blur',
    IMAGE_WINDOW_BLUR: 'image_window_blur',
    SELECTION_TOOLBAR_BLUR: 'selection_toolbar_blur',

    // AI 设置
    GET_AI_SETTINGS: 'get_ai_settings',
    SAVE_APP_SETTINGS: 'save_app_settings',
    TEST_AI_CONNECTION: 'test_ai_connection',
    GET_PROVIDER_CONFIG: 'get_provider_config',
    REMOVE_AI_PROVIDER: 'remove_ai_provider',
    GET_ALL_CONFIGURED_PROVIDERS: 'get_all_configured_providers',
    GET_POLL_METRICS_HISTORY: 'get_poll_metrics_history',
    GET_POLL_METRICS_MINUTE_AGGREGATES: 'get_poll_metrics_minute_aggregates',
    EXPORT_POLL_METRICS: 'export_poll_metrics',
    EXPORT_POLL_METRICS_TO_FILE: 'export_poll_metrics_to_file',
    GET_TEXT_DEDUP_METRICS: 'get_text_dedup_metrics',

    // AI 功能
    STREAM_TRANSLATE_TEXT: 'stream_translate_text',
    STREAM_EXPLAIN_TEXT: 'stream_explain_text',
};

/**
 * 剪贴板相关的 IPC 服务
 */
export const ClipboardService = {
    /**
     * 获取剪贴板历史记录
     * @returns {Promise<{history: string[], categories: Object, category_list: string[]}>}
     */
    getHistory: () => invoke(IPC_COMMANDS.GET_CLIPBOARD_HISTORY),

    /**
     * 删除剪贴板条目
     * @param {number} index
     * @returns {Promise<void>}
     */
    removeItem: (index) => invoke(IPC_COMMANDS.REMOVE_CLIPBOARD_ITEM, {index}),

    /**
     * 选择并填充内容
     * @param {number} index
     * @returns {Promise<void>}
     */
    selectAndFill: (index, opId) =>
        invoke(IPC_COMMANDS.SELECT_AND_FILL, {request: buildSelectAndFillRequest(index, opId)}),

    /**
     * 复制文本到剪贴板
     * @param {string} text
     * @returns {Promise<void>}
     */
    copyText: (text) => invoke(IPC_COMMANDS.COPY_TEXT, {text}),
    copyAndPasteText: (text) => invoke(IPC_COMMANDS.COPY_AND_PASTE_TEXT, {text}),
};

export const ImageClipboardService = {
    getHistory: () => invoke(IPC_COMMANDS.GET_IMAGE_CLIPBOARD_HISTORY),
    removeItem: (index) => invoke(IPC_COMMANDS.REMOVE_IMAGE_CLIPBOARD_ITEM, {index}),
    selectAndFill: (index, opId) =>
        invoke(IPC_COMMANDS.SELECT_AND_FILL_IMAGE, {request: buildSelectAndFillImageRequest(index, opId)}),
    warmupItem: (index) => invoke(IPC_COMMANDS.WARMUP_IMAGE_CLIPBOARD_ITEM, {index}),
    openPreviewWindow: (index) => invoke(IPC_COMMANDS.OPEN_IMAGE_PREVIEW_WINDOW, {index}),
    closePreviewWindow: () => invoke(IPC_COMMANDS.CLOSE_IMAGE_PREVIEW_WINDOW),
};

/**
 * 分类管理相关的 IPC 服务
 */
export const CategoryService = {
    /**
     * 设置条目分类
     * @param {string} item
     * @param {string} category
     * @returns {Promise<void>}
     */
    setItemCategory: (item, category) => invoke(IPC_COMMANDS.SET_ITEM_CATEGORY, {item, category}),

    /**
     * 删除分类
     * @param {string} category
     * @returns {Promise<void>}
     */
    removeCategory: (category) => invoke(IPC_COMMANDS.REMOVE_CATEGORY, {category}),

    /**
     * 添加分类
     * @param {string} category
     * @returns {Promise<void>}
     */
    addCategory: (category) => invoke(IPC_COMMANDS.ADD_CATEGORY, {category}),
};

export const ImageCategoryService = {
    setItemCategory: (itemId, category) => invoke(IPC_COMMANDS.SET_IMAGE_ITEM_CATEGORY, {itemId, category}),
    removeCategory: (category) => invoke(IPC_COMMANDS.REMOVE_IMAGE_CATEGORY, {category}),
    addCategory: (category) => invoke(IPC_COMMANDS.ADD_IMAGE_CATEGORY, {category}),
};

/**
 * 窗口管理相关的 IPC 服务
 */
export const WindowService = {
    /**
     * 获取窗口底部偏移量
     * @returns {Promise<number>}
     */
    getBottomOffset: () => invoke(IPC_COMMANDS.GET_CLIPBOARD_BOTTOM_OFFSET),

    /**
     * 预览窗口底部偏移量
     * @param {number} offset
     * @returns {Promise<void>}
     */
    previewBottomOffset: (offset) => invoke(IPC_COMMANDS.PREVIEW_CLIPBOARD_BOTTOM_OFFSET, {offset}),

    /**
     * 保存窗口底部偏移量
     * @param {number} offset
     * @returns {Promise<void>}
     */
    saveBottomOffset: (offset) => invoke(IPC_COMMANDS.SAVE_CLIPBOARD_BOTTOM_OFFSET, {offset}),

    /**
     * 窗口失去焦点通知
     * @returns {Promise<void>}
     */
    blur: () => invoke(IPC_COMMANDS.WINDOW_BLUR),
    imageBlur: () => invoke(IPC_COMMANDS.IMAGE_WINDOW_BLUR),

    /**
     * 选择工具栏失去焦点通知
     * @returns {Promise<void>}
     */
    selectionToolbarBlur: () => invoke(IPC_COMMANDS.SELECTION_TOOLBAR_BLUR),
};

/**
 * AI 设置相关的 IPC 服务
 */
export const AISettingsService = {
    /**
     * 获取 AI 设置
     * @returns {Promise<Object>}
     */
    getSettings: () => invoke(IPC_COMMANDS.GET_AI_SETTINGS),

    /**
     * 保存应用设置
     * @param {Object} params
     * @param {number} params.maxItems
     * @param {string} params.aiProvider
     * @param {string} params.aiApiUrl
     * @param {string} params.aiModelName
     * @param {string} params.aiApiKey
     * @param {string} params.hotKey
     * @param {string} params.imageHotKey
     * @param {boolean} params.selectionEnabled
     * @param {boolean} params.groupedItemsProtectedFromLimit
     * @param {string} params.translationPromptTemplate
     * @param {string} params.explanationPromptTemplate
     * @param {number} params.clipboardPollMinIntervalMs
     * @param {number} params.clipboardPollWarmIntervalMs
     * @param {number} params.clipboardPollIdleIntervalMs
     * @param {number} params.clipboardPollMaxIntervalMs
     * @param {number} params.clipboardPollReportIntervalSecs
     * @param {boolean} params.clipboardPollMetricsEnabled
     * @param {string} params.clipboardPollMetricsLogLevel
     * @returns {Promise<void>}
     */
    saveSettings: ({
                       maxItems,
                       aiProvider,
                       aiApiUrl,
                       aiModelName,
                       aiApiKey,
                       hotKey,
                       imageHotKey,
                       selectionEnabled,
                       groupedItemsProtectedFromLimit,
                       translationPromptTemplate,
                       explanationPromptTemplate,
                       clipboardPollMinIntervalMs,
                       clipboardPollWarmIntervalMs,
                       clipboardPollIdleIntervalMs,
                       clipboardPollMaxIntervalMs,
                       clipboardPollReportIntervalSecs,
                       clipboardPollMetricsEnabled,
                       clipboardPollMetricsLogLevel
                   }) =>
        invoke(IPC_COMMANDS.SAVE_APP_SETTINGS, {
            maxItems,
            aiProvider,
            aiApiUrl,
            aiModelName,
            aiApiKey,
            hotKey,
            imageHotKey,
            selectionEnabled,
            groupedItemsProtectedFromLimit,
            translationPromptTemplate,
            explanationPromptTemplate,
            clipboardPollMinIntervalMs,
            clipboardPollWarmIntervalMs,
            clipboardPollIdleIntervalMs,
            clipboardPollMaxIntervalMs,
            clipboardPollReportIntervalSecs,
            clipboardPollMetricsEnabled,
            clipboardPollMetricsLogLevel
        }),

    /**
     * 测试 AI 连接
     * @param {Object} params
     * @param {string} params.aiApiUrl
     * @param {string} params.aiModelName
     * @param {string} params.aiApiKey
     * @returns {Promise<string>}
     */
    testConnection: ({aiApiUrl, aiModelName, aiApiKey}) =>
        invoke(IPC_COMMANDS.TEST_AI_CONNECTION, {aiApiUrl, aiModelName, aiApiKey}),

    /**
     * 获取提供商配置
     * @param {string} provider
     * @returns {Promise<[string, string]>} [url, model]
     */
    getProviderConfig: (provider) => invoke(IPC_COMMANDS.GET_PROVIDER_CONFIG, {provider}),

    /**
     * 删除 AI 提供商
     * @param {string} provider
     * @returns {Promise<void>}
     */
    removeProvider: (provider) => invoke(IPC_COMMANDS.REMOVE_AI_PROVIDER, {provider}),

    /**
     * 获取所有已配置的提供商
     * @returns {Promise<Array<[string, string]>>}
     */
    getAllConfiguredProviders: () => invoke(IPC_COMMANDS.GET_ALL_CONFIGURED_PROVIDERS),
    getPollMetricsHistory: (limit = 120) =>
        invoke(IPC_COMMANDS.GET_POLL_METRICS_HISTORY, {limit}),
    getPollMetricsMinuteAggregates: (limitMinutes = 60) =>
        invoke(IPC_COMMANDS.GET_POLL_METRICS_MINUTE_AGGREGATES, {limitMinutes}),
    exportPollMetrics: (format = 'json', limit = 720) =>
        invoke(IPC_COMMANDS.EXPORT_POLL_METRICS, {format, limit}),
    exportPollMetricsToFile: ({format = 'json', limit = 720, filePath}) =>
        invoke(IPC_COMMANDS.EXPORT_POLL_METRICS_TO_FILE, {format, limit, filePath}),
    getTextDedupMetrics: () =>
        invoke(IPC_COMMANDS.GET_TEXT_DEDUP_METRICS),
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
    streamTranslate: (text, sourceLanguage, targetLanguage, opId, sceneHint) =>
        invoke(IPC_COMMANDS.STREAM_TRANSLATE_TEXT, {
            request: buildStreamTranslateRequest(text, sourceLanguage, targetLanguage, opId, sceneHint)
        }),

    /**
     * 流式解释文本
     * @param {string} text
     * @param {string} targetLanguage
     * @returns {Promise<void>}
     */
    streamExplain: (text, targetLanguage, opId, sceneHint) =>
        invoke(IPC_COMMANDS.STREAM_EXPLAIN_TEXT, {
            request: buildStreamExplainRequest(text, targetLanguage, opId, sceneHint)
        }),
};
