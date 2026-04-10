const envValue = String(import.meta.env.VITE_ENABLE_NATIVE_CONTEXT_MENU || '').toLowerCase()
const enableNativeContextMenu = envValue === 'true'

if (!enableNativeContextMenu) {
    const preventNativeContextMenu = (event) => {
        event.preventDefault()
    }
    window.addEventListener('contextmenu', preventNativeContextMenu, {capture: true})
}
