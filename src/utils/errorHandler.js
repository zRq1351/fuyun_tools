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
 * Parse and handle errors
 * @param {any} error - The caught error object
 * @param {string} context - Context where error occurred
 */
export function handleAppError(error, context = 'Operation failed') {
    console.error(`[${context}]`, error)

    const i18n = getI18nInstance()
    const t = i18n && i18n.global ? i18n.global.t.bind(i18n.global) : (key, params) => key

    let message = ''
    let code = null
    let details = null

    if (typeof error === 'object' && error !== null) {
        if (error.code && error.message) {
            code = error.code
            message = error.message
            details = error.details
        } else if (error.toString) {
            message = error.toString()
        }
    } else {
        message = String(error)
    }

    switch (code) {
        case ErrorCode.CONFIG_ERROR:
            ElMessage.error({
                message: t('errorHandler.configError', {context, message}),
                duration: 5000,
                showClose: true
            })
            break
        case ErrorCode.NETWORK_ERROR:
            ElMessage.error({
                message: t('errorHandler.networkError', {context}),
                duration: 5000,
                showClose: true
            })
            break
        case ErrorCode.VALIDATION_ERROR:
            ElMessage.warning({
                message: t('errorHandler.warning', {context, message}),
                duration: 3000,
                showClose: true
            })
            break
        default: {
            const lowerMsg = message.toLowerCase()
            if (lowerMsg.includes('未配置ai') || lowerMsg.includes('提供商') || lowerMsg.includes('no ai provider')) {
                ElMessage.error(t('errorHandler.noAIProvider'))
            } else if (lowerMsg.includes('api地址') || lowerMsg.includes('api地址不能为空') || lowerMsg.includes('no api url') || lowerMsg.includes('endpoint')) {
                ElMessage.error(t('errorHandler.noApiUrl'))
            } else if (lowerMsg.includes('api密钥') || lowerMsg.includes('api key') || lowerMsg.includes('no api key') || lowerMsg.includes('secret')) {
                ElMessage.error(t('errorHandler.noApiKey'))
            } else {
                ElMessage.error({
                    message: t('errorHandler.error', {context, message}),
                    duration: 5000,
                    showClose: true
                })
            }
        }
    }
}
