import js from '@eslint/js'
import pluginVue from 'eslint-plugin-vue'

export default [
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
                ResizeObserver: 'readonly',
                AbortController: 'readonly',
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
            'vue/no-mutating-props': 'off',
            'no-unused-vars': ['warn', {argsIgnorePattern: '^_'}],
            'no-console': ['warn', {allow: ['warn', 'error']}],
            'no-empty': ['warn', {allowEmptyCatch: true}],
            // IPC 命令名必须走 services/ipc.js 的 IPC_COMMANDS 常量表：
            // 避免字符串字面量拼写错误只能运行时暴露，同时保留统一错误上下文
            'no-restricted-syntax': [
                'error',
                {
                    selector: "CallExpression[callee.name='invoke'][arguments.0.type='Literal']",
                    message: '禁止裸字符串 invoke 调用，请使用 IPC_COMMANDS 常量：invoke(IPC_COMMANDS.XXX)。新命令先在 services/ipc.js 的 IPC_COMMANDS 中登记。',
                },
            ],
        },
    },
    {
        ignores: ['dist/**', 'node_modules/**', '*.html'],
    },
]
