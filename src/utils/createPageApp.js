import './disableContextMenu'
import {createApp} from 'vue'
import 'element-plus/dist/index.css'
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

function initLocaleListeners() {
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
    // 全局未捕获 Promise 异常处理
    window.addEventListener('unhandledrejection', (event) => {
        console.error('[UnhandledRejection]', event.reason)
        event.preventDefault()
    })

    initTheme()

    watchThemeStorage((theme) => {
        // themeManager already calls applyTheme internally
    })

    watchSystemTheme((theme) => {
        // themeManager already calls applyTheme internally
    })

    initLocaleListeners()

    const app = createApp(rootComponent)

    app.use(i18n)

    if (options.setup) {
        options.setup(app)
    }

    app.mount('#app')
    return app
}
