import {ElMessage} from 'element-plus'

/**
 * 错误代码常量，与 Rust 端保持一致
 */
export const ErrorCode = {
    CONFIG_ERROR: 'CONFIG_ERROR',
    NETWORK_ERROR: 'NETWORK_ERROR',
    IO_ERROR: 'IO_ERROR',
    CLIPBOARD_ERROR: 'CLIPBOARD_ERROR',
    SYSTEM_ERROR: 'SYSTEM_ERROR',
    VALIDATION_ERROR: 'VALIDATION_ERROR',
}

/**
 * 解析并处理错误
 * @param {any} error - 捕获的错误对象
 * @param {string} context - 错误发生的上下文（例如 "翻译失败"）
 */
export function handleAppError(error, context = '操作失败') {
    console.error(`[${context}]`, error)

    let message = ''
    let code = null
    let details = null

    // 尝试解析 AppError 结构
    if (typeof error === 'object' && error !== null) {
        // 检查是否是 Rust 返回的 AppError 结构
        if (error.code && error.message) {
            code = error.code
            message = error.message
            details = error.details
        } else if (error.toString) {
            // 处理普通 Error 对象或字符串错误
            message = error.toString()
        }
    } else {
        message = String(error)
    }

    // 根据错误代码提供更友好的提示
    switch (code) {
        case ErrorCode.CONFIG_ERROR:
            ElMessage.error({
                message: `${context}: 配置错误 - ${message}`,
                duration: 5000,
                showClose: true
            })
            break
        case ErrorCode.NETWORK_ERROR:
            ElMessage.error({
                message: `${context}: 网络连接失败，请检查网络设置`,
                duration: 5000,
                showClose: true
            })
            break
        case ErrorCode.VALIDATION_ERROR:
            ElMessage.warning({
                message: `${context}: ${message}`,
                duration: 3000,
                showClose: true
            })
            break
        default:
            // 对于普通字符串错误，尝试进行简单的关键词匹配（兼容旧代码或未捕获的 panic）
            if (message.includes('未配置AI提供商')) {
                ElMessage.error('未配置 AI 提供商，请在设置中填写 API Key 与 Endpoint 后重试。')
            } else if (message.includes('API地址不能为空')) {
                ElMessage.error('API地址未配置，请在设置中填写正确的API地址。')
            } else if (message.includes('API密钥未配置')) {
                ElMessage.error('API密钥未配置，请在设置中填写正确的API密钥。')
            } else {
                ElMessage.error({
                    message: `${context}: ${message}`,
                    duration: 5000,
                    showClose: true
                })
            }
    }
}
