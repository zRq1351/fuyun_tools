import js from '@eslint/js'
import pluginVue from 'eslint-plugin-vue'

export default [
    {
        ignores: ['dist/**', 'node_modules/**', '*.html'],
    },
    js.configs.recommended,
    ...pluginVue.configs['flat/recommended'],
    {
        languageOptions: {
            globals: {
                // Browser globals
                window: 'readonly',
                document: 'readonly',
                console: 'readonly',
                localStorage: 'readonly',
                sessionStorage: 'readonly',
                setTimeout: 'readonly',
                clearTimeout: 'readonly',
                setInterval: 'readonly',
                clearInterval: 'readonly',
                requestAnimationFrame: 'readonly',
                cancelAnimationFrame: 'readonly',
                alert: 'readonly',
                confirm: 'readonly',
                prompt: 'readonly',
                navigator: 'readonly',
                performance: 'readonly',
                atob: 'readonly',
                btoa: 'readonly',
                URL: 'readonly',
                Image: 'readonly',
                Blob: 'readonly',
                fetch: 'readonly',
                createImageBitmap: 'readonly',
                miniIcon: 'readonly',
                // DOM types
                Element: 'readonly',
                HTMLElement: 'readonly',
                MouseEvent: 'readonly',
                KeyboardEvent: 'readonly',
                DragEvent: 'readonly',
                ClipboardEvent: 'readonly',
                CustomEvent: 'readonly',
                StorageEvent: 'readonly',
                Node: 'readonly',
                ClipboardItem: 'readonly',
                ImageData: 'readonly',
                ResizeObserver: 'readonly',
                AbortController: 'readonly',
                AbortSignal: 'readonly',
                // Node.js globals (for Tauri/Vite)
                __dirname: 'readonly',
                __filename: 'readonly',
                // Project-specific globals
                __DEV_PANEL__: 'readonly',
                // Tauri API
                __TAURI__: 'readonly',
            },
        },
        rules: {
            'vue/multi-word-component-names': 'off',
            'no-unused-vars': ['warn', {
                argsIgnorePattern: '^_',
                caughtErrorsIgnorePattern: '^_',
                varsIgnorePattern: '^_',
                ignoreRestSiblings: true,
            }],
            'no-console': ['warn', {allow: ['warn', 'error', 'debug', 'info']}],
            'no-empty': ['warn', {allowEmptyCatch: true}],
        },
    },
    {
        // 设置页通过共享 form 对象收集字段，子组件直接写 form.xxx 是既有架构
        files: ['pages/settings/**/*.vue'],
        rules: {
            'vue/no-mutating-props': 'off',
        },
    },
    {
        // 工具栏/剪贴板等组件大量函数 prop，默认值由父组件始终传入
        files: [
            'pages/clipboard/**/*.vue',
            'pages/image_clipboard/**/*.vue',
            'pages/launcher/**/*.vue',
            'pages/recording_toolbar/**/*.vue',
            'pages/document_manager_widget/**/*.vue',
            'pages/screenshot/**/*.vue',
        ],
        rules: {
            'vue/require-default-prop': 'off',
        },
    },
]
