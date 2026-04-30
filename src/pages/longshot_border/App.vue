<template>
  <div class="frame"></div>
</template>

<style scoped>
:global(html),
:global(body),
:global(#app) {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: transparent;
}
.frame {
  width: 100%;
  height: 100%;
  border: 2px solid var(--fy-accent);
  box-sizing: border-box;
  border-radius: 2px;
  background: transparent;
}
</style>
