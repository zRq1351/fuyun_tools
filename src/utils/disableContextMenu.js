const preventNativeContextMenu = (event) => {
  event.preventDefault()
}

window.addEventListener('contextmenu', preventNativeContextMenu, {capture: true})
