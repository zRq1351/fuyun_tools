/**
 * 浮云工具 - 主题管理器
 * 支持：暗色(dark) / 亮色(light) / 护眼(eye-care)
 *
 * 主题唯一存储：Rust 后端 settings.json（通过 get_theme / set_theme IPC 命令）
 * localStorage 仅作为同步缓存，不作为持久存储
 */

import {invoke} from '@tauri-apps/api/core'

const THEME_KEY = 'fuyun-theme'
const THEMES = ['dark', 'light', 'eye-care']
const THEME_LABELS = {
    dark: '暗色',
    light: '亮色',
    'eye-care': '护眼'
}

/**
 * 获取当前主题（从后端异步读取，同步回退到 localStorage）
 */
export async function fetchTheme() {
    try {
        const theme = await invoke('get_theme')
        if (THEMES.includes(theme)) {
            localStorage.setItem(THEME_KEY, theme)
            return theme
        }
    } catch (e) {
        // 后端不可用时使用 localStorage 缓存
        console.debug('[ThemeManager] 从后端获取主题失败，使用缓存:', e?.message || e)
    }
    return getTheme()
}

/**
 * 获取当前主题（同步，从 localStorage 缓存读取）
 */
export function getTheme() {
    const saved = localStorage.getItem(THEME_KEY)
    if (THEMES.includes(saved)) return saved
    return 'dark'
}

/**
 * 设置主题（写入后端 + 更新 localStorage 缓存）
 */
export function setTheme(theme) {
    if (!THEMES.includes(theme)) {
        console.warn(`[ThemeManager] 无效的主题: ${theme}`)
        return
    }
    localStorage.setItem(THEME_KEY, theme)
    applyTheme(theme)
    // 带重试的后端保存
    const saveWithRetry = (retries = 2) => {
        invoke('set_theme', {theme}).catch(err => {
            console.warn('[ThemeManager] 保存主题到后端失败:', err)
            if (retries > 0) {
                setTimeout(() => saveWithRetry(retries - 1), 1000)
            }
        })
    }
    saveWithRetry()
    window.dispatchEvent(new CustomEvent('theme-change', {detail: {theme}}))
}

/**
 * 应用主题到 DOM
 */
export function applyTheme(theme) {
    const validTheme = THEMES.includes(theme) ? theme : 'dark'
    const html = document.documentElement
    html.classList.remove('dark', 'light', 'eye-care')
    html.removeAttribute('data-theme')
    html.classList.add(validTheme)
    html.setAttribute('data-theme', validTheme)
    if (validTheme === 'dark') {
        html.classList.add('dark')
    }
    document.body.classList.remove('theme-dark', 'theme-light', 'theme-eye-care')
    document.body.classList.add(`theme-${validTheme}`)
}

export function resetToSystemTheme() {
    localStorage.removeItem(THEME_KEY)
    applyTheme(getSystemTheme())
    invoke('set_theme', {theme: getSystemTheme()}).catch(err => {
        console.debug('[ThemeManager] 重置主题到系统主题失败:', err?.message || err)
    })
}

function getSystemTheme() {
    return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function watchSystemTheme(callback) {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = (e) => {
        if (!localStorage.getItem(THEME_KEY)) {
            const theme = e.matches ? 'dark' : 'light'
            applyTheme(theme)
            callback?.(theme)
        }
    }
    mediaQuery.addEventListener('change', handler)
    return () => mediaQuery.removeEventListener('change', handler)
}

export function watchThemeStorage(callback) {
    const handler = (e) => {
        if (e.key === THEME_KEY) {
            const theme = e.newValue || getSystemTheme()
            applyTheme(theme)
            callback?.(theme)
        }
    }
    window.addEventListener('storage', handler)
    return () => window.removeEventListener('storage', handler)
}

export function watchThemeChange(callback) {
    const handler = (e) => {
        callback?.(e.detail.theme)
    }
    window.addEventListener('theme-change', handler)
    return () => window.removeEventListener('theme-change', handler)
}

/**
 * 初始化主题：先用 localStorage 快速应用，再从后端同步
 */
export function initTheme() {
    const saved = localStorage.getItem(THEME_KEY)
    const theme = THEMES.includes(saved) ? saved : 'dark'
    applyTheme(theme)
    // 异步从后端同步最新主题
    fetchTheme().then(backendTheme => {
        if (backendTheme && backendTheme !== theme) {
            applyTheme(backendTheme)
            localStorage.setItem(THEME_KEY, backendTheme)
            window.dispatchEvent(new CustomEvent('theme-change', {detail: {theme: backendTheme}}))
        }
    }).catch(err => {
        console.debug('[ThemeManager] 从后端同步主题失败:', err?.message || err)
    })
    return theme
}

export function getAvailableThemes() {
    return THEMES.map(value => ({
        value,
        label: THEME_LABELS[value] || value
    }))
}

export function isDarkTheme() {
    return getTheme() === 'dark'
}
