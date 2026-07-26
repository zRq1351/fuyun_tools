<template>
  <div :class="contentType" class="formatted-content" @click="handleClick">
    <!-- eslint-disable vue/no-v-html -->
    <div v-if="contentType === 'markdown'" class="markdown-body" v-html="renderedHtml"></div>
    <pre v-else-if="contentType === 'code'"><code class="hljs" v-html="renderedHtml"></code></pre>
    <div v-else class="plain-text">{{ content }}</div>
  </div>
</template>

<script setup>
import {ref, watch, onBeforeUnmount} from 'vue'
import {Marked} from 'marked'
import {markedHighlight} from 'marked-highlight'
import DOMPurify from 'dompurify'
import hljs from 'highlight.js'
import {openUrl} from '@tauri-apps/plugin-opener'
import 'highlight.js/styles/atom-one-dark.css'

const DOMPURIFY_CONFIG = {
  USE_PROFILES: {html: true},
  ADD_ATTR: ['target'],
  FORBID_TAGS: ['style'],
}

const props = defineProps({
  content: {
    type: String,
    required: true
  }
})

const handleClick = (event) => {
  const anchor = event.target.closest('a')
  if (!anchor) return
  const href = anchor.getAttribute('href')
  if (!href) return
  event.preventDefault()
  event.stopPropagation()
  openUrl(href).catch(() => {
    window.open(href, '_blank', 'noopener,noreferrer')
  })
}

const marked = new Marked(
  markedHighlight({
    emptyLangClass: 'hljs',
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = hljs.getLanguage(lang) ? lang : 'plaintext'
      return hljs.highlight(code, {language}).value
    }
  }),
)

const contentType = ref('text')
const renderedHtml = ref('')
let pendingRafId = null

const RENDER_CACHE_MAX_SIZE = 200
const renderCache = new Map()

const getCachedResult = (text) => {
  const cached = renderCache.get(text)
  if (cached) {
    renderCache.delete(text)
    renderCache.set(text, cached)
    return cached
  }
  return null
}

const setCacheResult = (text, result) => {
  if (renderCache.size >= RENDER_CACHE_MAX_SIZE) {
    const firstKey = renderCache.keys().next().value
    renderCache.delete(firstKey)
  }
  renderCache.set(text, result)
}

const detectAndProcess = (text) => {
  const trimmed = text.trim()
  if (!trimmed) {
    contentType.value = 'text'
    renderedHtml.value = ''
    return
  }

  if (text.length > 50000) {
    contentType.value = 'text'
    renderedHtml.value = ''
    return
  }

  const cached = getCachedResult(text)
  if (cached) {
    contentType.value = cached.type
    renderedHtml.value = cached.html
    return
  }

  // 1. JSON
  if (
      (trimmed.startsWith('{') && trimmed.endsWith('}')) ||
      (trimmed.startsWith('[') && trimmed.endsWith(']'))
  ) {
    try {
      JSON.parse(trimmed)
      const rawHtml = hljs.highlight(text, {language: 'json'}).value
      const html = DOMPurify.sanitize(rawHtml, DOMPURIFY_CONFIG)
      contentType.value = 'code'
      renderedHtml.value = html
      setCacheResult(text, {type: 'code', html})
      return
    } catch (e) {
      // not json
    }
  }

  // 2. HTML
  if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
    const rawHtml = hljs.highlight(text, {language: 'html'}).value
    const html = DOMPurify.sanitize(rawHtml, DOMPURIFY_CONFIG)
    contentType.value = 'code'
    renderedHtml.value = html
    setCacheResult(text, {type: 'code', html})
    return
  }

  // 3. Markdown (simplified detection)
  if (/(^|\n)(#{1,6}\s|- \[[ x]\]|[*-]\s|> \S|```)/.test(text)) {
    try {
      const rawHtml = marked.parse(text)
      const html = DOMPurify.sanitize(rawHtml, DOMPURIFY_CONFIG)
      contentType.value = 'markdown'
      renderedHtml.value = html
      setCacheResult(text, {type: 'markdown', html})
    } catch (e) {
      contentType.value = 'text'
      renderedHtml.value = text
      setCacheResult(text, {type: 'text', html: text})
    }
    return
  }

  // 4. Code heuristics
  const codeRegex =
      /(function\s+\w+\(|const\s+\w+\s*=|let\s+\w+\s*=|var\s+\w+\s*=|class\s+\w+\s*\{|import\s+.*from|public\s+class|def\s+\w+\(|fn\s+\w+\()/
  if (codeRegex.test(text)) {
    try {
      const rawHtml = hljs.highlightAuto(text).value
      const html = DOMPurify.sanitize(rawHtml, DOMPURIFY_CONFIG)
      contentType.value = 'code'
      renderedHtml.value = html
      setCacheResult(text, {type: 'code', html})
    } catch (e) {
      contentType.value = 'text'
      renderedHtml.value = ''
      setCacheResult(text, {type: 'text', html: ''})
    }
    return
  }

  // Default
  contentType.value = 'text'
  renderedHtml.value = ''
  setCacheResult(text, {type: 'text', html: ''})
}

watch(() => props.content, (newVal) => {
  if (pendingRafId !== null) cancelAnimationFrame(pendingRafId)
  pendingRafId = requestAnimationFrame(() => {
    detectAndProcess(newVal || '')
    pendingRafId = null
  })
}, { immediate: true })

onBeforeUnmount(() => {
  if (pendingRafId !== null) {
    cancelAnimationFrame(pendingRafId)
    pendingRafId = null
  }
})

</script>

<style scoped>
.formatted-content {
  font-size: 13px;
  line-height: 1.5;
  color: var(--fy-text-primary);
  width: 100%;
}

.plain-text {
  white-space: pre-wrap;
  word-break: break-all;
}

/* Markdown specific styles */
.markdown-body {
  word-break: break-word;
}
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4),
.markdown-body :deep(h5),
.markdown-body :deep(h6) {
  margin-top: 0;
  margin-bottom: 8px;
  font-weight: 600;
  line-height: 1.25;
}
.markdown-body :deep(h1) { font-size: 1.2em; }
.markdown-body :deep(h2) { font-size: 1.15em; }
.markdown-body :deep(h3) { font-size: 1.1em; }
.markdown-body :deep(p) {
  margin-top: 0;
  margin-bottom: 8px;
}
.markdown-body :deep(p:last-child) {
  margin-bottom: 0;
}
.markdown-body :deep(a) {
  color: var(--fy-accent);
  text-decoration: none;
}
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin-top: 0;
  margin-bottom: 8px;
  padding-left: 20px;
}
.markdown-body :deep(code) {
  background-color: var(--fy-bg-hover);
  padding: 2px 4px;
  border-radius: 4px;
  font-family: monospace;
}
.markdown-body :deep(pre) {
  background-color: var(--fy-bg-surface);
  padding: 8px;
  border-radius: 6px;
  overflow-x: auto;
  margin-top: 0;
  margin-bottom: 8px;
}
.markdown-body :deep(pre code) {
  background-color: transparent;
  padding: 0;
}
.markdown-body :deep(blockquote) {
  margin: 0 0 8px 0;
  padding-left: 10px;
  border-left: 3px solid var(--fy-border-light);
  color: var(--fy-text-muted);
}

/* Code specific styles */
pre {
  margin: 0;
  background-color: var(--fy-bg-surface);
  padding: 8px;
  border-radius: 6px;
  overflow-x: auto;
}
pre code {
  font-family: monospace;
  font-size: 12px;
}
</style>
