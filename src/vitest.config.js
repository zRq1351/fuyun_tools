import {defineConfig} from 'vitest/config'

export default defineConfig({
    // 与 vite.config.js 保持一致：ipc.js 顶层引用该编译期常量
    define: {
        __DEV_PANEL__: false,
    },
    test: {
        environment: 'node',
        // clipboard-logic.test.js 是 node:test 编写，由 node --test 运行，不纳入 vitest
        include: [
            'tests/{errorHandler,fileUrl,localeManager}.test.js',
            'tests/{screenshotAnnotation,formatDisplay,diagnosticStatus}.test.js',
            'tests/{i18nKeys,launcherSearch}.test.js',
            'tests/clipboardRegression.test.js'
        ],
        globals: false,
    },
})
