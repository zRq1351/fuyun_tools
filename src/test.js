import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const { wrapperProps } = useVirtualList(ref(Array(100).fill(1)), { itemWidth: 20 })
console.log(wrapperProps.value)
