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
                image_clipboard: resolve(__dirname, 'image_clipboard.html'),
                image_preview: resolve(__dirname, 'image_preview.html'),
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
