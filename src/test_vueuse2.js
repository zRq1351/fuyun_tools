import { useVirtualList } from '@vueuse/core';
import { ref } from 'vue';

const items = ref(Array.from({ length: 50 }, (_, i) => i));
const { list, containerProps, wrapperProps } = useVirtualList(items, { itemWidth: 258, overscan: 10 });

const container = { clientWidth: 800, scrollLeft: 12000, scrollTop: 0 };
containerProps.ref.value = container;
containerProps.onScroll({ target: container });

console.log(wrapperProps.value.style);
