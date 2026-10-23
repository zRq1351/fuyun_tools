import './disableContextMenu'
import {createApp} from 'vue'
// element-plus 样式由 unplugin 的 ElementPlusResolver 按组件按需注入（importStyle: 'css'），
// 不再全量引入 dist/index.css（358KB），每个窗口只加载实际使用的组件样式
import 'element-plus/theme-chalk/dark/css-vars.css'
import '../pages/shared/theme-variables.css'
import '../pages/shared/windowBase.css'
import {initTheme, watchThemeStorage, watchSystemTheme} from './themeManager'
import {createI18nInstance, getLocale, getI18nInstance} from './localeManager'
import zhCN from '../locales/zh-CN.json'
import enUS from '../locales/en-US.json'

const i18n = createI18nInstance({
    'zh-CN': zhCN,
    'en-US': enUS
})

// 全局单例：防止多窗口重复注册
let _globalRejectionHandlerRegistered = false
let _localeListenersRegistered = false

function ensureGlobalRejectionHandler() {
    if (_globalRejectionHandlerRegistered) return
    _globalRejectionHandlerRegistered = true
    window.addEventListener('unhandledrejection', (event) => {
        console.error('[UnhandledRejection]', event.reason)
        event.preventDefault()
    })
}

function ensureLocaleListeners() {
    if (_localeListenersRegistered) return
    _localeListenersRegistered = true
    const handler = () => {
        const inst = getI18nInstance()
        if (inst && inst.global) {
            inst.global.locale.value = getLocale()
        }
        document.documentElement.setAttribute('lang', getLocale())
    }
    window.addEventListener('locale-change', handler)
    window.addEventListener('storage', (e) => {
        if (e.key === 'fuyun-locale') {
            handler()
        }
    })
}

/**
 * Create a standard page application factory function
 * @param {Object} rootComponent - Vue root component
 * @param {Object} [options] - Optional config
 * @param {Function} [options.setup] - Extra setup callback after app creation, receives app instance
 * @returns {Object} Vue application instance
 */
export function createPageApp(rootComponent, options = {}) {
    ensureGlobalRejectionHandler()

    initTheme()

    watchThemeStorage((theme) => {
        // themeManager already calls applyTheme internally
    })

    watchSystemTheme((theme) => {
        // themeManager already calls applyTheme internally
    })

    ensureLocaleListeners()

    const app = createApp(rootComponent)

    app.use(i18n)

    if (options.setup) {
        options.setup(app)
    }

    app.mount('#app')
    return app
}
