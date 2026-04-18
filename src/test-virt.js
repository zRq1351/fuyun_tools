import { ref } from 'vue'
import { useVirtualList } from '@vueuse/core'

const list = ref(Array.from({ length: 21 }, (_, i) => i))
const { containerProps, wrapperProps } = useVirtualList(list, { itemWidth: 258 })

console.log('containerProps:', containerProps)
console.log('wrapperProps.value:', wrapperProps.value)
