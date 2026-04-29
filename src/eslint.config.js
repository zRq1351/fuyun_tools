import js from '@eslint/js'
import pluginVue from 'eslint-plugin-vue'

export default [
    js.configs.recommended,
    ...pluginVue.configs['flat/recommended'],
    {
        rules: {
            'vue/multi-word-component-names': 'off',
            'vue/no-v-html': 'off',
            'no-unused-vars': ['warn', {argsIgnorePattern: '^_'}],
            'no-console': ['warn', {allow: ['warn', 'error']}],
        },
    },
    {
        ignores: ['dist/**', 'node_modules/**', '*.html'],
    },
]
