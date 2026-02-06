import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
import {resolve} from 'path'

export default defineConfig({
    plugins: [vue()],
    build: {
        rollupOptions: {
            input: {
                settings: resolve(__dirname, 'settings.html'),
                clipboard: resolve(__dirname, 'clipboard.html'),
                selection_toolbar: resolve(__dirname, 'selection_toolbar.html'),
                result_display: resolve(__dirname, 'result_display.html'),
            },
            output: {
                manualChunks: {
                    'element-plus': ['element-plus'],
                    'vue': ['vue'],
                }
            }
        },
        outDir: 'dist',
        emptyOutDir: true,
        chunkSizeWarningLimit: 2000,
    },
    resolve: {
        alias: {
            '@': resolve(__dirname, '.'),
        },
    },
})
