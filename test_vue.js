const { compileTemplate } = require('@vue/compiler-sfc');
const code = `
<template>
  <div v-bind="containerProps" @scroll="handleScroll"></div>
</template>
`;
const result = compileTemplate({ source: code, id: 'test' });
console.log(result.code);
