import {describe, it, expect, vi, beforeEach} from 'vitest'

const mockConvertFileSrc = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({
    convertFileSrc: (...args) => mockConvertFileSrc(...args),
}))

import {buildFileUrlFromPath} from '../utils/fileUrl.js'

describe('buildFileUrlFromPath', () => {
    beforeEach(() => {
        mockConvertFileSrc.mockReset()
    })

    it('空路径返回空串', () => {
        expect(buildFileUrlFromPath('')).toBe('')
        expect(buildFileUrlFromPath(null)).toBe('')
        expect(buildFileUrlFromPath(undefined)).toBe('')
    })

    it('有效路径转换为 asset URL', () => {
        mockConvertFileSrc.mockReturnValue('asset://C:/images/a.png')
        expect(buildFileUrlFromPath('C:/images/a.png')).toBe('asset://C:/images/a.png')
        expect(mockConvertFileSrc).toHaveBeenCalledWith('C:/images/a.png')
    })

    it('convertFileSrc 抛错时返回空串', () => {
        mockConvertFileSrc.mockImplementation(() => {
            throw new Error('fail')
        })
        expect(buildFileUrlFromPath('C:/bad.png')).toBe('')
    })
})
