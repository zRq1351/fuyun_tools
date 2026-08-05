import {describe, it, expect} from 'vitest'
import {parseFrontendErrorJson, parseLegacyError, parseErrorMessage, ErrorCode} from '../utils/errorHandler.js'

describe('parseFrontendErrorJson', () => {
    it('解析合法的前端错误 JSON', () => {
        const raw = JSON.stringify({
            code: 'E_CLIPBOARD_ITEM_NOT_FOUND',
            category: 'CLIPBOARD_ERROR',
            message: '找不到目标项目'
        })
        const parsed = parseFrontendErrorJson(raw)
        expect(parsed.code).toBe('E_CLIPBOARD_ITEM_NOT_FOUND')
        expect(parsed.message).toBe('找不到目标项目')
    })

    it('解析带 params 的错误 JSON', () => {
        const raw = JSON.stringify({
            code: 'E_SETTINGS_HOTKEY_CONFLICT',
            message: '快捷键被占用',
            params: {key: 'Alt+R'}
        })
        const parsed = parseFrontendErrorJson(raw)
        expect(parsed.params.key).toBe('Alt+R')
    })

    it('非 E_ 前缀的 JSON 返回 null', () => {
        const raw = JSON.stringify({code: 'NOT_ERROR', message: 'x'})
        expect(parseFrontendErrorJson(raw)).toBeNull()
    })

    it('非法 JSON 返回 null', () => {
        expect(parseFrontendErrorJson('{bad json')).toBeNull()
    })

    it('非字符串输入返回 null', () => {
        expect(parseFrontendErrorJson(null)).toBeNull()
        expect(parseFrontendErrorJson(undefined)).toBeNull()
        expect(parseFrontendErrorJson(123)).toBeNull()
    })

    it('普通字符串返回 null', () => {
        expect(parseFrontendErrorJson('plain text')).toBeNull()
    })
})

describe('parseLegacyError', () => {
    it('解析旧格式错误', () => {
        const parsed = parseLegacyError('[SYSTEM_ERROR] 系统出错')
        expect(parsed.category).toBe('SYSTEM_ERROR')
        expect(parsed.message).toBe('系统出错')
        expect(parsed.details).toBe('')
    })

    it('解析带详情（中文分号分隔）的旧格式', () => {
        const parsed = parseLegacyError('[IO_ERROR] 读取失败；path not found')
        expect(parsed.category).toBe('IO_ERROR')
        expect(parsed.message).toBe('读取失败')
        expect(parsed.details).toBe('path not found')
    })

    it('非旧格式返回 null', () => {
        expect(parseLegacyError('随便一句话')).toBeNull()
        expect(parseLegacyError('[UNKNOWN_CODE] x')).toBeNull()
        expect(parseLegacyError('')).toBeNull()
    })
})

describe('parseErrorMessage', () => {
    it('空输入返回空串', () => {
        expect(parseErrorMessage('')).toBe('')
        expect(parseErrorMessage(null)).toBe('')
    })

    it('JSON 格式提取 message（无 i18n 时回退 message）', () => {
        const raw = JSON.stringify({code: 'E_TEST', message: '测试消息'})
        expect(parseErrorMessage(raw)).toBe('测试消息')
    })

    it('旧格式提取 message', () => {
        expect(parseErrorMessage('[CLIPBOARD_ERROR] 剪贴板失败')).toBe('剪贴板失败')
    })

    it('未知格式原样返回', () => {
        expect(parseErrorMessage('原始错误文本')).toBe('原始错误文本')
    })
})

describe('ErrorCode 常量', () => {
    it('包含所有后端分类', () => {
        expect(ErrorCode.CONFIG_ERROR).toBe('CONFIG_ERROR')
        expect(ErrorCode.NETWORK_ERROR).toBe('NETWORK_ERROR')
        expect(ErrorCode.IO_ERROR).toBe('IO_ERROR')
        expect(ErrorCode.CLIPBOARD_ERROR).toBe('CLIPBOARD_ERROR')
        expect(ErrorCode.SYSTEM_ERROR).toBe('SYSTEM_ERROR')
        expect(ErrorCode.VALIDATION_ERROR).toBe('VALIDATION_ERROR')
    })
})
