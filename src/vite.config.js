import {defineConfig} from 'vite'
import vue from '@vitejs/plugin-vue'
import {resolve} from 'path'

export default defineConfig(({mode}) => {
    const isDevMode = mode === 'development'
    return {
        plugins: [vue()],
        define: {
            __DEV_PANEL__: JSON.stringify(isDevMode),
        },
        build: {
            rollupOptions: {
                input: {
                    settings: resolve(__dirname, 'settings.html'),
                    clipboard: resolve(__dirname, 'clipboard.html'),
                    image_clipboard: resolve(__dirname, 'image_clipboard.html'),
                    image_preview: resolve(__dirname, 'image_preview.html'),
                    selection_toolbar: resolve(__dirname, 'selection_toolbar.html'),
                    result_display: resolve(__dirname, 'result_display.html'),
                    screenshot: resolve(__dirname, 'screenshot.html'),
                    recording_toolbar: resolve(__dirname, 'recording_toolbar.html'),
                    pinned_image: resolve(__dirname, 'pinned_image.html'),
                    ocr_text: resolve(__dirname, 'ocr_text.html'),
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
                '@dev/DeveloperSettings': isDevMode
                    ? resolve(__dirname, 'pages/settings/components/DeveloperSettings.vue')
                    : resolve(__dirname, 'pages/settings/components/DeveloperSettings.stub.vue'),
            },
        },
    }
})
