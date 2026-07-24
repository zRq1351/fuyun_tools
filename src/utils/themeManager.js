/**
 * 浮云工具 - 主题管理器
 * 支持：暗色(dark) / 亮色(light) / 护眼(eye-care)
 *
 * 基于 Element Plus 官方暗黑模式实现
 * 使用 .dark 类激活 Element Plus 暗黑模式变量
 */

const THEME_KEY = 'fuyun-theme'
const THEMES = ['dark', 'light', 'eye-care']
const THEME_LABELS = {
    dark: '暗色',
    light: '亮色',
    'eye-care': '护眼'
}

/**
 * 获取当前主题
 * @returns {string} 主题名称
 */
export function getTheme() {
    const saved = localStorage.getItem(THEME_KEY)
    if (THEMES.includes(saved)) return saved
    return 'dark'
}

/**
 * 设置主题
 * @param {string} theme - 主题名称
 */
export function setTheme(theme) {
    if (!THEMES.includes(theme)) {
        console.warn(`[ThemeManager] 无效的主题: ${theme}`)
        return
    }
    console.log('[ThemeManager] setTheme:', theme)
    localStorage.setItem(THEME_KEY, theme)
    applyTheme(theme)
    // 触发自定义事件供同窗口监听
    window.dispatchEvent(new CustomEvent('theme-change', {detail: {theme}}))
    // 触发 storage 事件供跨窗口同步
    try {
        window.dispatchEvent(new StorageEvent('storage', {
            key: THEME_KEY,
            newValue: theme,
            oldValue: null,
            storageArea: localStorage
        }))
    } catch (e) {
        // 某些浏览器可能不允许手动触发 storage 事件
    }
}

/**
 * 应用主题到 DOM
 * 使用 Element Plus 官方的 .dark 类切换方式
 * @param {string} theme - 主题名称
 */
export function applyTheme(theme) {
    const validTheme = THEMES.includes(theme) ? theme : 'dark'
    const html = document.documentElement

    // 移除所有主题类
    html.classList.remove('dark', 'light', 'eye-care')
    html.removeAttribute('data-theme')

    // 添加当前主题类
    html.classList.add(validTheme)
    html.setAttribute('data-theme', validTheme)

    // Element Plus 使用 .dark 类来激活暗黑模式
    // 只有 dark 主题需要添加 .dark 类
    if (validTheme === 'dark') {
        html.classList.add('dark')
    }

    // 同步 body 的 class
    document.body.classList.remove('theme-dark', 'theme-light', 'theme-eye-care')
    document.body.classList.add(`theme-${validTheme}`)
}

/**
 * 重置为系统主题
 */
export function resetToSystemTheme() {
    localStorage.removeItem(THEME_KEY)
    applyTheme(getSystemTheme())
}

/**
 * 监听系统主题变化
 * @param {Function} callback - 主题变化回调
 * @returns {Function} 清理函数
 */
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

/**
 * 监听跨窗口主题变化
 * @param {Function} callback - 主题变化回调
 * @returns {Function} 清理函数
 */
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

/**
 * 监听同窗口主题变化
 * @param {Function} callback - 主题变化回调
 * @returns {Function} 清理函数
 */
export function watchThemeChange(callback) {
    const handler = (e) => {
        callback?.(e.detail.theme)
    }
    window.addEventListener('theme-change', handler)
    return () => window.removeEventListener('theme-change', handler)
}

/**
 * 初始化主题（在页面加载时调用）
 * @returns {string} 当前主题
 */
export function initTheme() {
    const saved = localStorage.getItem(THEME_KEY)
    const theme = THEMES.includes(saved) ? saved : 'dark'
    console.log('[ThemeManager] initTheme:', {saved, theme})
    applyTheme(theme)
    return theme
}

/**
 * 获取所有可用主题
 * @returns {Array<{value: string, label: string}>}
 */
export function getAvailableThemes() {
    return THEMES.map(value => ({
        value,
        label: THEME_LABELS[value] || value
    }))
}

/**
 * 检查是否为暗色主题
 * @returns {boolean}
 */
export function isDarkTheme() {
    return getTheme() === 'dark'
}
