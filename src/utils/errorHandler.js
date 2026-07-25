/**
 * Error code constants, matching Rust backend
 */
import {ElMessage} from 'element-plus'
import {getI18nInstance} from './localeManager'

export const ErrorCode = {
    CONFIG_ERROR: 'CONFIG_ERROR',
    NETWORK_ERROR: 'NETWORK_ERROR',
    IO_ERROR: 'IO_ERROR',
    CLIPBOARD_ERROR: 'CLIPBOARD_ERROR',
    SYSTEM_ERROR: 'SYSTEM_ERROR',
    VALIDATION_ERROR: 'VALIDATION_ERROR',
}

/**
 * Try to parse a backend error as JSON FrontendErrorPayload
 * @param {string} raw - The raw error string from Tauri
 * @returns {{code: string, message: string, params?: object}|null}
 */
function parseFrontendErrorJson(raw) {
    if (typeof raw !== 'string') return null
    try {
        const parsed = JSON.parse(raw)
        if (parsed && typeof parsed.code === 'string' && parsed.code.startsWith('E_')) {
            return parsed
        }
    } catch (_e) {
        // Not JSON, handle as legacy format
    }
    return null
}

/**
 * Parse legacy string error format: "[CODE] message；details"
 */
function parseLegacyError(raw) {
    const match = raw.match(/^\[(CONFIG_ERROR|NETWORK_ERROR|IO_ERROR|CLIPBOARD_ERROR|SYSTEM_ERROR|VALIDATION_ERROR)\]\s*(.+)/)
    if (match) {
        const parts = match[2].split('；')
        return {
            category: match[1],
            message: parts[0].trim(),
            details: parts.slice(1).join('；').trim()
        }
    }
    return null
}

/**
 * Parse and handle errors from Rust backend
 * Supports both new JSON FrontendErrorPayload format and legacy [CODE] format
 * @param {any} error - The caught error (string from Tauri invoke rejection)
 * @param {string} context - Context where error occurred (already i18n-translated by caller)
 */
export function handleAppError(error, context = 'Operation failed') {
    const i18n = getI18nInstance()
    const t = i18n && i18n.global ? i18n.global.t.bind(i18n.global) : (key, params) => key

    const raw = typeof error === 'string' ? error : String(error || '')
    console.error(`[${context}]`, raw)

    // Try new JSON format first
    const jsonError = parseFrontendErrorJson(raw)
    if (jsonError) {
        const i18nKey = `errorCodes.${jsonError.code}`
        const fallbackMsg = jsonError.message || raw
        // Try to look up i18n key; if not found, use the fallback message
        const translated = t(i18nKey, jsonError.params || {})
        const displayMsg = translated === i18nKey ? fallbackMsg : translated
        ElMessage.error({
            message: displayMsg,
            duration: 5000,
            showClose: true
        })
        return
    }

    // Try legacy [CODE] format
    const legacy = parseLegacyError(raw)
    if (legacy) {
        switch (legacy.category) {
            case 'CONFIG_ERROR':
                ElMessage.error({
                    message: t('errorHandler.configError', {context, message: legacy.message}),
                    duration: 5000,
                    showClose: true
                })
                return
            case 'NETWORK_ERROR':
                ElMessage.error({
                    message: t('errorHandler.networkError', {context}),
                    duration: 5000,
                    showClose: true
                })
                return
            case 'VALIDATION_ERROR':
                ElMessage.warning({
                    message: t('errorHandler.warning', {context, message: legacy.message}),
                    duration: 3000,
                    showClose: true
                })
                return
            case 'IO_ERROR':
            case 'CLIPBOARD_ERROR':
            case 'SYSTEM_ERROR':
            default:
                // Check for known patterns in legacy messages (中文 + English)
                const lowerMsg = legacy.message.toLowerCase()
                if (lowerMsg.includes('未配置ai') || lowerMsg.includes('ai not configured') || lowerMsg.includes('no ai provider') || (lowerMsg.includes('ai') && lowerMsg.includes('提供商'))) {
                    ElMessage.error(t('errorHandler.noAIProvider'))
                    return
                }
                if (lowerMsg.includes('api地址') || lowerMsg.includes('api url') || lowerMsg.includes('api地址未配置')) {
                    ElMessage.error(t('errorHandler.noApiUrl'))
                    return
                }
                if (lowerMsg.includes('api密钥') || lowerMsg.includes('api key') || lowerMsg.includes('api密钥未配置')) {
                    ElMessage.error(t('errorHandler.noApiKey'))
                    return
                }
                ElMessage.error({
                    message: t('errorHandler.error', {context, message: legacy.message}),
                    duration: 5000,
                    showClose: true
                })
                return
        }
    }

    // Raw/unknown format - keyword matching (中文 + English)
    const lowerMsg = raw.toLowerCase()
    if (lowerMsg.includes('未配置ai') || lowerMsg.includes('ai not configured') || lowerMsg.includes('no ai provider') || (lowerMsg.includes('ai') && lowerMsg.includes('提供商'))) {
        ElMessage.error(t('errorHandler.noAIProvider'))
    } else if (lowerMsg.includes('api地址') || lowerMsg.includes('api url') || lowerMsg.includes('api地址未配置')) {
        ElMessage.error(t('errorHandler.noApiUrl'))
    } else if (lowerMsg.includes('api密钥') || lowerMsg.includes('api key') || lowerMsg.includes('api密钥未配置')) {
        ElMessage.error(t('errorHandler.noApiKey'))
    } else {
        ElMessage.error({
            message: t('errorHandler.error', {context, message: raw}),
            duration: 5000,
            showClose: true
        })
    }
}

/**
 * Simple helper: parse error and return a user-friendly message string
 * Useful for inline notices or when you just need the text
 * @param {string} raw - The raw error string from Tauri
 * @returns {string}
 */
export function parseErrorMessage(raw) {
    const i18n = getI18nInstance()
    const t = i18n && i18n.global ? i18n.global.t.bind(i18n.global) : (key, params) => key
    if (!raw) return ''

    const jsonError = parseFrontendErrorJson(raw)
    if (jsonError) {
        const i18nKey = `errorCodes.${jsonError.code}`
        const translated = t(i18nKey, jsonError.params || {})
        return translated === i18nKey ? (jsonError.message || raw) : translated
    }

    const legacy = parseLegacyError(raw)
    if (legacy) {
        return legacy.message
    }

    return raw
}
