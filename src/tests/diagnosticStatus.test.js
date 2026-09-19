import {describe, it, expect} from 'vitest'
import {statusType, statusI18nKey} from '../utils/diagnosticStatus.js'

describe('statusType', () => {
  it('healthy/warning/error 映射到 Element Plus tag 类型', () => {
    expect(statusType('healthy')).toBe('success')
    expect(statusType('warning')).toBe('warning')
    expect(statusType('error')).toBe('danger')
  })

  it('未知状态为 info', () => {
    expect(statusType('unknown')).toBe('info')
    expect(statusType('')).toBe('info')
    expect(statusType(null)).toBe('info')
  })
})

describe('statusI18nKey', () => {
  it('各状态返回对应 diagnostic key', () => {
    expect(statusI18nKey('healthy')).toBe('settings.diagnostic.statusNormal')
    expect(statusI18nKey('warning')).toBe('settings.diagnostic.statusWarning')
    expect(statusI18nKey('error')).toBe('settings.diagnostic.statusError')
  })

  it('未知状态返回 statusUnknown', () => {
    expect(statusI18nKey('whatever')).toBe('settings.diagnostic.statusUnknown')
    expect(statusI18nKey(undefined)).toBe('settings.diagnostic.statusUnknown')
  })
})
