import {invoke} from '@tauri-apps/api/core';

const buildSelectAndFillRequest = (index, opId) => ({index, opId});
const buildSelectAndFillImageByIdRequest = (itemId, opId) => ({itemId, opId});
const normalizeOptionalIndex = (index) =>
    Number.isInteger(index) && index >= 0 ? index : null;
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
    SELECT_AND_FILL_IMAGE_BY_ID: 'select_and_fill_image_by_id',
    WARMUP_IMAGE_CLIPBOARD_ITEM_BY_ID: 'warmup_image_clipboard_item_by_id',
    WARMUP_MULTIPLE_IMAGES: 'warmup_multiple_images',
    OPEN_IMAGE_PREVIEW_WINDOW_BY_ID: 'open_image_preview_window_by_id',
    CLOSE_IMAGE_PREVIEW_WINDOW: 'close_image_preview_window',
    COPY_TEXT: 'copy_text',
    COPY_AND_PASTE_TEXT: 'copy_and_paste_text',

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

    // AI 设置
    GET_AI_SETTINGS: 'get_ai_settings',
    SAVE_APP_SETTINGS: 'save_app_settings',
    TEST_AI_CONNECTION: 'test_ai_connection',
    GET_PROVIDER_CONFIG: 'get_provider_config',
    REMOVE_AI_PROVIDER: 'remove_ai_provider',
    GET_ALL_CONFIGURED_PROVIDERS: 'get_all_configured_providers',
    GET_TEXT_DEDUP_METRICS: 'get_text_dedup_metrics',
    GET_IMAGE_STORAGE_METRICS: 'get_image_storage_metrics',

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
    getFullSnapshot: () => invoke('get_clipboard_full_snapshot'),
    getHistoryPage: ({
                         offset = 0,
                         limit = 50,
                         category = null,
                         pinnedOnly = false,
                         keyword = null,
                         sortBy = null,
                         sortOrder = null
                     } = {}) =>
        invoke(IPC_COMMANDS.GET_CLIPBOARD_HISTORY_PAGE, {
            request: {offset, limit, category, pinnedOnly, keyword, sortBy, sortOrder}
        }),

    /**
     * 删除剪贴板条目
     * @param {number} index
     * @returns {Promise<void>}
     */
    removeItem: (index, item = null) => invoke(IPC_COMMANDS.REMOVE_CLIPBOARD_ITEM, {
        index: normalizeOptionalIndex(index),
        item
    }),
    setItemPinned: (index, item, pinned) => invoke(IPC_COMMANDS.SET_CLIPBOARD_ITEM_PINNED, {
        index: normalizeOptionalIndex(index),
        item,
        pinned
    }),
    promoteItem: (index) => invoke(IPC_COMMANDS.PROMOTE_CLIPBOARD_ITEM, {index}),
    clearHistory: (mode) => invoke(IPC_COMMANDS.CLEAR_TEXT_HISTORY, {mode}),

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
    getHistoryPage: ({
                         offset = 0,
                         limit = 50,
                         category = null,
                         keyword = null,
                         pinnedOnly = false,
                         sortBy = 'pinnedFirst',
                         sortOrder = null
                     } = {}) =>
        invoke(IPC_COMMANDS.GET_IMAGE_CLIPBOARD_HISTORY_PAGE, {
            request: {offset, limit, category, keyword, pinnedOnly, sortBy, sortOrder}
        }),
    removeItemById: (itemId) => invoke(IPC_COMMANDS.REMOVE_IMAGE_CLIPBOARD_ITEM_BY_ID, {itemId}),
    promoteItemById: (itemId) =>
        invoke(IPC_COMMANDS.PROMOTE_IMAGE_CLIPBOARD_ITEM_BY_ID, {request: {itemId}}),
    setItemPinned: (itemId, pinned) => invoke(IPC_COMMANDS.SET_IMAGE_ITEM_PINNED, {itemId, pinned}),
    clearHistory: (mode) => invoke(IPC_COMMANDS.CLEAR_IMAGE_HISTORY, {mode}),
    importImageFiles: (paths) => invoke(IPC_COMMANDS.IMPORT_IMAGE_FILES, {paths}),
    selectAndFillById: (itemId, opId) =>
        invoke(IPC_COMMANDS.SELECT_AND_FILL_IMAGE_BY_ID, {
            request: buildSelectAndFillImageByIdRequest(itemId, opId)
        }),
    warmupItemById: (itemId) =>
        invoke(IPC_COMMANDS.WARMUP_IMAGE_CLIPBOARD_ITEM_BY_ID, {request: {itemId}}),
    warmupMultipleItems: (itemIds) =>
        invoke(IPC_COMMANDS.WARMUP_MULTIPLE_IMAGES, {itemIds}),
    openPreviewWindowById: (itemId) =>
        invoke(IPC_COMMANDS.OPEN_IMAGE_PREVIEW_WINDOW_BY_ID, {request: {itemId}}),
    closePreviewWindow: () => invoke(IPC_COMMANDS.CLOSE_IMAGE_PREVIEW_WINDOW),
    setItemTags: (itemId, tags) => invoke(IPC_COMMANDS.SET_IMAGE_ITEM_TAGS, {itemId, tags}),

    // 异步预览相关方法
    getImagePreviewById: (itemId) =>
        invoke(IPC_COMMANDS.GET_IMAGE_PREVIEW_BY_ID, {itemId}),
    checkPreviewsReady: (itemIds) =>
        invoke(IPC_COMMANDS.CHECK_PREVIEWS_READY, {itemIds}),
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
     * @param {number} params.textMaxItems
     * @param {number} params.imageMaxItems
     * @param {number} params.imageDiskLimitMb
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
     * @param {string} params.imageFillVerifyMode
     * @returns {Promise<void>}
     */
    saveSettings: ({
                       textMaxItems,
                       imageMaxItems,
                       imageDiskLimitMb,
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
                       imageFillVerifyMode
                   }) =>
        invoke(IPC_COMMANDS.SAVE_APP_SETTINGS, {
            textMaxItems,
            imageMaxItems,
            imageDiskLimitMb,
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
            imageFillVerifyMode
        }),

    /**
     * 部分保存应用设置（只保存变化的字段）
     * @param {Object} changedFields - 变化的字段对象
     * @returns {Promise<void>}
     */
    savePartialSettings: (changedFields) =>
        invoke(IPC_COMMANDS.SAVE_APP_SETTINGS, changedFields),

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
    getTextDedupMetrics: () =>
        invoke(IPC_COMMANDS.GET_TEXT_DEDUP_METRICS),
    getImageStorageMetrics: () =>
        invoke(IPC_COMMANDS.GET_IMAGE_STORAGE_METRICS),
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
