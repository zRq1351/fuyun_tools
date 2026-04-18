const puppeteer = require('puppeteer');

(async () => {
  const browser = await puppeteer.launch({ args: ['--no-sandbox'] });
  const page = await browser.newPage();
  await page.setContent(`
    <!DOCTYPE html>
    <html>
    <head>
    <style>
    .content {
      display: flex;
      flex-direction: row;
      width: 800px;
      overflow-x: auto;
      border: 1px solid black;
      padding: 8px 8px 8px 8px;
      box-sizing: border-box;
    }
    .virtual-wrapper {
      display: flex;
      flex-direction: row;
      height: 100%;
    }
    .item {
      width: 250px;
      margin-right: 8px;
      background: rgba(0, 0, 0, 0.6);
      flex-shrink: 0;
      border: 1px solid red;
      box-sizing: border-box;
    }
    .tail {
      flex-shrink: 0;
      width: 56px;
      flex: 0 0 56px;
      background: red;
      margin-right: 8px;
    }
    .content::after {
      content: '';
      flex-shrink: 0;
      width: 1px;
    }
    </style>
    </head>
    <body>
    <div class="content" id="c">
      <div class="virtual-wrapper" id="v" style="width: 2580px; margin-left: 10320px;">
         <div class="item">40</div>
         <div class="item">41</div>
         <div class="item">42</div>
         <div class="item">43</div>
         <div class="item">44</div>
         <div class="item">45</div>
         <div class="item">46</div>
         <div class="item">47</div>
         <div class="item">48</div>
         <div class="item" id="item49">49</div>
      </div>
      <div class="tail" id="tail">Tail</div>
    </div>
    </body>
    </html>
  `);
  
  const result = await page.evaluate(() => {
    const c = document.getElementById('c');
    c.scrollLeft = 100000;
    return {
      scrollWidth: c.scrollWidth,
      scrollLeft: c.scrollLeft,
      clientWidth: c.clientWidth,
      tailOffsetRight: document.getElementById('tail').offsetLeft + document.getElementById('tail').offsetWidth
    };
  });
  console.log(result);
  await browser.close();
})();
