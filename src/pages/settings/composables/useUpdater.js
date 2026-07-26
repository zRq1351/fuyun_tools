import {h, ref} from 'vue'
import {useI18n} from 'vue-i18n'
import {check} from '@tauri-apps/plugin-updater'
import {relaunch} from '@tauri-apps/plugin-process'
import {Marked} from 'marked'
import {ElMessageBox} from 'element-plus'

const marked = new Marked({
    breaks: false,
    gfm: true,
})

const safeText = (value) => (typeof value === 'string' ? value : '')

const isSafeHref = (href) => typeof href === 'string' && /^(https?:\/\/|mailto:)/i.test(href)

const renderInlineTokens = (tokens = []) => {
    const nodes = []
    for (const token of tokens) {
        if (!token) continue
        if (token.type === 'text') {
            nodes.push(safeText(token.text))
            continue
        }
        if (token.type === 'strong') {
            nodes.push(h('strong', null, renderInlineTokens(token.tokens || [])))
            continue
        }
        if (token.type === 'em') {
            nodes.push(h('em', null, renderInlineTokens(token.tokens || [])))
            continue
        }
        if (token.type === 'codespan') {
            nodes.push(h('code', null, safeText(token.text)))
            continue
        }
        if (token.type === 'br') {
            nodes.push(h('br'))
            continue
        }
        if (token.type === 'del') {
            nodes.push(h('del', null, renderInlineTokens(token.tokens || [])))
            continue
        }
        if (token.type === 'link') {
            const href = isSafeHref(token.href) ? token.href : '#'
            nodes.push(
                h(
                    'a',
                    {href, target: '_blank', rel: 'noopener noreferrer'},
                    renderInlineTokens(token.tokens || [{type: 'text', text: safeText(token.text)}])
                )
            )
            continue
        }
        nodes.push(safeText(token.raw || token.text))
    }
    return nodes
}

const renderBlockToken = (token, index) => {
    if (!token) return null
    if (token.type === 'paragraph') {
        return h('p', {key: `p-${index}`}, renderInlineTokens(token.tokens || []))
    }
    if (token.type === 'heading') {
        const level = Math.min(6, Math.max(1, Number(token.depth) || 1))
        return h(`h${level}`, {key: `h-${index}`}, renderInlineTokens(token.tokens || []))
    }
    if (token.type === 'list') {
        const tag = token.ordered ? 'ol' : 'ul'
        return h(
            tag,
            {key: `l-${index}`},
            (token.items || []).map((item, itemIndex) =>
                h('li', {key: `li-${index}-${itemIndex}`}, renderInlineTokens(item.tokens || []))
            )
        )
    }
    if (token.type === 'blockquote') {
        return h(
            'blockquote',
            {key: `bq-${index}`},
            (token.tokens || []).map((child, childIndex) => renderBlockToken(child, `${index}-${childIndex}`))
        )
    }
    if (token.type === 'code') {
        return h('pre', {key: `code-${index}`}, [h('code', null, safeText(token.text))])
    }
    if (token.type === 'hr') {
        return h('hr', {key: `hr-${index}`})
    }
    if (token.type === 'space') {
        return h('div', {key: `sp-${index}`})
    }
    return h('p', {key: `x-${index}`}, safeText(token.raw || token.text))
}

const buildUpdateMessageNode = (version, body, t) => {
    const raw = safeText(body).trim()
    const tokens = raw ? marked.lexer(raw) : [{
        type: 'paragraph',
        tokens: [{type: 'text', text: t('updater.noUpdateInfo')}]
    }]
    const renderedBlocks = tokens
        .map((token, index) => renderBlockToken(token, index))
        .filter(Boolean)
    return h('div', {class: 'update-dialog'}, [
        h('div', {class: 'update-dialog__hero'}, [
            h('div', {class: 'update-dialog__tag'}, t('updater.newVersion')),
            h('h3', {class: 'update-dialog__title'}, `v${version}`)
        ]),
        h('div', {class: 'update-body-content'}, renderedBlocks),
        h('p', {class: 'update-dialog__hint'}, t('updater.updateNow'))
    ])
}

export function useUpdater(currentVersion) {
    const {t} = useI18n()
    const checkingUpdate = ref(false)
    const updateStatus = ref(null)
    const updateProgress = ref(0)
    const showUpdateProgress = ref(false)

    const checkUpdate = async () => {
        checkingUpdate.value = true
        updateStatus.value = {message: t('updater.status.checking'), type: 'info'}
        showUpdateProgress.value = false
        updateProgress.value = 0

        try {
            const update = await check()
            if (update) {
                updateStatus.value = null

                try {
                    const messageNode = buildUpdateMessageNode(update.version, update.body, t)

                    await ElMessageBox.confirm(
                        messageNode,
                        t('updater.foundUpdate'),
                        {
                            confirmButtonText: t('updater.updateImmediately'),
                            cancelButtonText: t('updater.remindLater'),
                            type: 'primary',
                            customClass: 'update-message-box'
                        }
                    )

                    showUpdateProgress.value = true
                    updateStatus.value = {message: t('updater.status.downloading'), type: 'info'}

                    let contentLength = 0
                    let downloaded = 0

                    await update.downloadAndInstall((event) => {
                        if (event.event === 'Started') {
                            contentLength = event.data.contentLength || 0
                            downloaded = 0
                            updateProgress.value = 0
                        } else if (event.event === 'Progress') {
                            downloaded += event.data.chunkLength
                            if (contentLength > 0) {
                                updateProgress.value = Math.round((downloaded / contentLength) * 100)
                            }
                        } else if (event.event === 'Finished') {
                            updateProgress.value = 100
                        }
                    })

                    updateStatus.value = {message: t('updater.status.downloadComplete'), type: 'success'}

                    await ElMessageBox.confirm(
                        t('updater.restartPrompt'),
                        t('updater.updateComplete'),
                        {
                            confirmButtonText: t('updater.restartNow'),
                            cancelButtonText: t('updater.restartLater'),
                            type: 'success',
                        }
                    )

                    await relaunch()

                } catch (action) {
                    if (action === 'cancel') {
                        updateStatus.value = {message: t('updater.status.cancelled'), type: 'info'}
                    }
                }
            } else {
                updateStatus.value = {message: t('updater.status.alreadyLatest'), type: 'success'}
            }
        } catch (error) {
            if (error !== 'cancel') {
                updateStatus.value = {message: t('updater.status.networkError'), type: 'error'}
            }
        } finally {
            checkingUpdate.value = false
        }
    }

    return {
        checkingUpdate,
        updateStatus,
        updateProgress,
        showUpdateProgress,
        checkUpdate
    }
}
