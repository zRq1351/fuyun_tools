export function statusType(status) {
  if (status === 'healthy') return 'success'
  if (status === 'warning') return 'warning'
  if (status === 'error') return 'danger'
  return 'info'
}

export function statusI18nKey(status) {
  if (status === 'healthy') return 'settings.diagnostic.statusNormal'
  if (status === 'warning') return 'settings.diagnostic.statusWarning'
  if (status === 'error') return 'settings.diagnostic.statusError'
  return 'settings.diagnostic.statusUnknown'
}
