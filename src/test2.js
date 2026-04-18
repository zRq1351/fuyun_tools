import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const { containerProps, wrapperProps } = useVirtualList(ref(Array(100).fill(1)), { itemWidth: 20 })

// Simulate scrolling by manually calling the onScroll handler
containerProps.onScroll({ target: { scrollLeft: 500, scrollTop: 0 } })

console.log(wrapperProps.value)
