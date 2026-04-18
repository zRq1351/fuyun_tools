import { useVirtualList } from '@vueuse/core'
import { ref } from 'vue'

const { wrapperProps } = useVirtualList(ref([]), { itemWidth: 258 })
console.log(wrapperProps.value)
