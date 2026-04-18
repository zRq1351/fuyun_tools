const { JSDOM } = require('jsdom')

const dom = new JSDOM(`
<!DOCTYPE html>
<html>
<head>
<style>
.content {
  display: flex;
  flex-direction: row;
  padding: 8px;
  width: 1000px;
  overflow-x: auto;
}
.virtual-wrapper {
  display: flex;
  flex-direction: row;
  height: 100%;
  flex-shrink: 0;
  width: 5418px;
  margin-left: 0px;
}
.clipboard-item {
  width: 250px;
  margin-right: 8px;
  flex-shrink: 0;
  box-sizing: border-box;
  border: 1px solid red;
}
.indicator {
  width: 56px;
  flex-shrink: 0;
}
</style>
</head>
<body>
  <div class="content" id="content">
    <div class="virtual-wrapper" id="wrapper">
      ${Array.from({length: 21}, (_, i) => `<div class="clipboard-item">${i}</div>`).join('')}
    </div>
    <div class="indicator"></div>
  </div>
</body>
</html>
`)

console.log(dom.window.document.getElementById('content').scrollWidth)
