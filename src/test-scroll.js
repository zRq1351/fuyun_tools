import { createSSRApp } from 'vue'
import { renderToString } from 'vue/server-renderer'

const app = createSSRApp({
  setup() {
    return {
      props: { onScroll: () => console.log('v-bind onScroll') },
      handleScroll: () => console.log('template onScroll')
    }
  },
  template: `<div v-bind="props" @scroll="handleScroll"></div>`
})

renderToString(app).then(html => console.log(html))
