<template>
  <div class="formatted-content" :class="contentType">
    <div v-if="contentType === 'markdown'" class="markdown-body" v-html="renderedHtml"></div>
    <pre v-else-if="contentType === 'code'"><code class="hljs" v-html="renderedHtml"></code></pre>
    <div v-else class="plain-text">{{ content }}</div>
  </div>
</template>

<script setup>
import { ref, watch } from 'vue'
import { Marked } from 'marked'
import { markedHighlight } from 'marked-highlight'
import DOMPurify from 'dompurify'
import hljs from 'highlight.js'
import 'highlight.js/styles/atom-one-dark.css'

const props = defineProps({
  content: {
    type: String,
    required: true
  }
})

const marked = new Marked(
  markedHighlight({
    emptyLangClass: 'hljs',
    langPrefix: 'hljs language-',
    highlight(code, lang) {
      const language = hljs.getLanguage(lang) ? lang : 'plaintext';
      return hljs.highlight(code, { language }).value;
    }
  })
);

const contentType = ref('text')
const renderedHtml = ref('')

const detectAndProcess = (text) => {
  const trimmed = text.trim()
  if (!trimmed) {
    contentType.value = 'text'
    renderedHtml.value = ''
    return
  }

  // Prevent freezing on very large texts
  if (text.length > 50000) {
    contentType.value = 'text'
    renderedHtml.value = ''
    return
  }

  // 1. JSON
  if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || 
      (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
    try {
      JSON.parse(trimmed)
      contentType.value = 'code'
      renderedHtml.value = hljs.highlight(text, { language: 'json' }).value
      return
    } catch (e) {
      // not json
    }
  }

  // 2. HTML
  if (trimmed.startsWith('<') && trimmed.endsWith('>')) {
    contentType.value = 'code'
    renderedHtml.value = hljs.highlight(text, { language: 'html' }).value
    return
  }

  // 3. Markdown (simplified detection)
  if (/(^|\n)(#{1,6}\s|- \[[ x]\]|[\*\-]\s|> \S|```)/.test(text)) {
    contentType.value = 'markdown'
    const rawHtml = marked.parse(text)
    // sanitize
    renderedHtml.value = DOMPurify.sanitize(rawHtml)
    return
  }

  // 4. Code heuristics
  const codeRegex = /(function\s+\w+\(|const\s+\w+\s*=|let\s+\w+\s*=|var\s+\w+\s*=|class\s+\w+\s*\{|import\s+.*from|public\s+class|def\s+\w+\(|fn\s+\w+\()/
  if (codeRegex.test(text)) {
    contentType.value = 'code'
    try {
      renderedHtml.value = hljs.highlightAuto(text).value
    } catch (e) {
      contentType.value = 'text'
    }
    return
  }

  // Default
  contentType.value = 'text'
  renderedHtml.value = ''
}

watch(() => props.content, (newVal) => {
  detectAndProcess(newVal || '')
}, { immediate: true })

</script>

<style scoped>
.formatted-content {
  font-size: 13px;
  line-height: 1.5;
  color: #dcdfe6;
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
  color: var(--el-color-primary, #409eff);
  text-decoration: none;
}
.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  margin-top: 0;
  margin-bottom: 8px;
  padding-left: 20px;
}
.markdown-body :deep(code) {
  background-color: rgba(255, 255, 255, 0.1);
  padding: 2px 4px;
  border-radius: 4px;
  font-family: monospace;
}
.markdown-body :deep(pre) {
  background-color: rgba(0, 0, 0, 0.3);
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
  border-left: 3px solid rgba(255, 255, 255, 0.2);
  color: rgba(255, 255, 255, 0.6);
}

/* Code specific styles */
pre {
  margin: 0;
  background-color: rgba(0, 0, 0, 0.3);
  padding: 8px;
  border-radius: 6px;
  overflow-x: auto;
}
pre code {
  font-family: monospace;
  font-size: 12px;
}
</style>
