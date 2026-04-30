import {computed, onUnmounted, ref} from 'vue'
import {ElMessage} from 'element-plus'

export function useShortcutRecorder(form, fieldKey = 'toggleShortcut') {
    const isRecording = ref(false)
    const recordedShortcut = ref('')
    const displayValue = ref('')

    const currentDisplayValue = computed(() => {
        if (isRecording.value) {
            return displayValue.value || '请按下快捷键...'
        }
        return form[fieldKey] || ''
    })

    const stopRecording = () => {
        isRecording.value = false
        document.removeEventListener('keydown', handleKeyDown, true)
        if (recordedShortcut.value) {
            form[fieldKey] = recordedShortcut.value
        } else {
        }
    }

    const handleKeyDown = (event) => {
        if (!isRecording.value) return
        event.preventDefault()
        event.stopPropagation()
        if (event.repeat) return

        const modifiers = []
        if (event.ctrlKey) modifiers.push('Ctrl')
        if (event.altKey) modifiers.push('Alt')
        if (event.shiftKey) modifiers.push('Shift')

        let key = ''
        if (event.code === 'Space') {
            key = 'Space'
        } else if (event.key.length === 1 && /[a-zA-Z0-9]/.test(event.key)) {
            key = event.key.toUpperCase()
        } else {
            const k = event.key.toLowerCase()
            const keyMap = {
                ' ': 'Space',
                'spacebar': 'Space',
                'enter': 'Enter',
                'tab': 'Tab',
                'backspace': 'Backspace',
                'delete': 'Delete',
                'escape': 'Escape',
                'esc': 'Escape',
                'arrowup': 'Up', 'up': 'Up',
                'arrowdown': 'Down', 'down': 'Down',
                'arrowleft': 'Left', 'left': 'Left',
                'arrowright': 'Right', 'right': 'Right'
            }
            if (keyMap[k]) {
                key = keyMap[k]
            } else if (k.startsWith('f') && k.length <= 3) {
                key = k.toUpperCase()
            }
        }

        if (modifiers.length > 0 && key) {
            recordedShortcut.value = [...modifiers, key].join('+')
            form[fieldKey] = recordedShortcut.value
            stopRecording()
            ElMessage.success(`已录制快捷键: ${recordedShortcut.value}`)
        }
    }

    const startRecording = () => {
        isRecording.value = true
        recordedShortcut.value = ''
        displayValue.value = '请按下快捷键...'
        document.addEventListener('keydown', handleKeyDown, true)
        ElMessage.info('开始录制快捷键，请按下组合键')
    }

    const toggleRecording = () => {
        if (isRecording.value) {
            stopRecording()
        } else {
            startRecording()
        }
    }

    onUnmounted(() => {
        stopRecording()
    })

    return {
        isRecording,
        currentDisplayValue,
        toggleRecording,
        startRecording,
        stopRecording
    }
}
