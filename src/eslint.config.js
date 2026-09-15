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
            'no-unused-vars': ['warn', {argsIgnorePattern: '^_'}],
            'no-console': ['warn', {allow: ['warn', 'error']}],
            'no-empty': ['warn', {allowEmptyCatch: true}],
        },
    },
    {
        ignores: ['dist/**', 'node_modules/**', '*.html'],
    },
]
