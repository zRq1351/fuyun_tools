import puppeteer from 'puppeteer';
(async () => {
  const browser = await puppeteer.launch({ args: ['--no-sandbox'] });
  const page = await browser.newPage();
  await page.goto("file:///workspace/test.html");
  const logs = await page.evaluate(() => {
    return {
      w: document.getElementById("wrapper").getBoundingClientRect().width,
      sw: document.querySelector(".content").scrollWidth
    }
  });
  console.log(logs);
  await browser.close();
})();
