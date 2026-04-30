/**
 * 浮云工具 - 主题管理 Vue Composable
 * 提供响应式主题状态和切换方法
 */

import {computed, onMounted, onUnmounted, ref} from 'vue'
import {
    getAvailableThemes,
    getTheme,
    resetToSystemTheme,
    setTheme,
    watchSystemTheme,
    watchThemeChange,
    watchThemeStorage
} from '@/utils/themeManager.js'

/**
 * 主题管理 Composable
 * @param {Object} options - 配置选项
 * @param {boolean} options.syncSystem - 是否监听系统主题变化（默认 true）
 * @param {boolean} options.syncStorage - 是否监听跨窗口主题变化（默认 true）
 * @param {Function} options.onChange - 主题变化回调
 * @returns {{ currentTheme: Ref<string>, changeTheme: Function, resetTheme: Function, isDark: ComputedRef<boolean>, themes: Array }}
 */
export function useTheme(options = {}) {
    const {
        syncSystem = true,
        syncStorage = true,
        onChange
    } = options

    const currentTheme = ref(getTheme())
    const themes = getAvailableThemes()

    const isDark = computed(() => currentTheme.value === 'dark')
    const isLight = computed(() => currentTheme.value === 'light')
    const isEyeCare = computed(() => currentTheme.value === 'eye-care')

    let cleanupFunctions = []

    function handleChange(theme) {
        currentTheme.value = theme
        onChange?.(theme)
    }

    function changeTheme(theme) {
        setTheme(theme)
        currentTheme.value = theme
    }

    function resetTheme() {
        resetToSystemTheme()
        currentTheme.value = getTheme()
    }

    function cycleTheme() {
        const order = ['dark', 'light', 'eye-care']
        const currentIndex = order.indexOf(currentTheme.value)
        const nextIndex = (currentIndex + 1) % order.length
        changeTheme(order[nextIndex])
    }

    onMounted(() => {
        // 初始化主题
        const theme = getTheme()
        currentTheme.value = theme

        // 监听系统主题变化
        if (syncSystem) {
            cleanupFunctions.push(watchSystemTheme(handleChange))
        }

        // 监听跨窗口主题变化
        if (syncStorage) {
            cleanupFunctions.push(watchThemeStorage(handleChange))
        }

        // 监听同窗口主题变化
        cleanupFunctions.push(watchThemeChange(handleChange))
    })

    onUnmounted(() => {
        cleanupFunctions.forEach(fn => fn())
        cleanupFunctions = []
    })

    return {
        currentTheme,
        isDark,
        isLight,
        isEyeCare,
        themes,
        changeTheme,
        resetTheme,
        cycleTheme
    }
}

/**
 * 简化版主题 Composable（仅提供当前主题状态）
 */
export function useThemeState() {
    const currentTheme = ref(getTheme())

    let cleanup

    onMounted(() => {
        cleanup = watchThemeChange((theme) => {
            currentTheme.value = theme
        })
    })

    onUnmounted(() => {
        cleanup?.()
    })

    return {
        currentTheme,
        isDark: computed(() => currentTheme.value === 'dark'),
        isLight: computed(() => currentTheme.value === 'light'),
        isEyeCare: computed(() => currentTheme.value === 'eye-care')
    }
}

