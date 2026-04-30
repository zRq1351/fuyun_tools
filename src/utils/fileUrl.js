import {convertFileSrc} from '@tauri-apps/api/core'

/**
 * 将本地文件路径转换为可加载的 URL
 * @param {string} imagePath - 本地文件路径
 * @returns {string} 可加载的 URL，失败时返回空字符串
 */
export function buildFileUrlFromPath(imagePath) {
    if (!imagePath) return ''
    try {
        return convertFileSrc(imagePath)
    } catch (_) {
        return ''
    }
}
