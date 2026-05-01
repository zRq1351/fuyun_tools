import './disableContextMenu'
import {createApp} from 'vue'
import 'element-plus/dist/index.css'
import 'element-plus/theme-chalk/dark/css-vars.css'
import '../pages/shared/theme-variables.css'
import '../pages/shared/windowBase.css'
import {initTheme, watchThemeStorage, watchSystemTheme} from './themeManager'

/**
 * 创建标准页面应用的工厂函数
 * @param {Object} rootComponent - Vue 根组件
 * @param {Object} [options] - 可选配置
 * @param {Function} [options.setup] - 应用创建后的额外设置回调，接收 app 实例
 * @returns {Object} Vue 应用实例
 */
export function createPageApp(rootComponent, options = {}) {
    initTheme()

    // 监听跨窗口主题变化，确保所有窗口同步更新
    watchThemeStorage((theme) => {
        // themeManager 内部已经调用了 applyTheme，无需额外操作
    })

    // 监听系统主题变化
    watchSystemTheme((theme) => {
        // themeManager 内部已经调用了 applyTheme，无需额外操作
    })

    const app = createApp(rootComponent)

    if (options.setup) {
        options.setup(app)
    }

    app.mount('#app')
    return app
}
