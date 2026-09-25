const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ channel: 'msedge', headless: true });
  try {
    const page = await browser.newPage();
    for (const width of [1366, 390]) {
      await page.setViewportSize({ width, height: 850 });
      await page.setContent(`<div class="modal_backdrop"><section class="customer_dialog product_editor_dialog">
        <header class="product_editor_header"><h2>Edit Item</h2><button aria-label="Close">X</button></header>
        <div class="product_editor_body"><div class="product_editor_main"><section class="product_editor_section">
        <h3>Product information</h3><div class="customer_form">
        <label>Product name *<input value="Sample product"></label>
        <label>Product type<select><option>Each</option></select></label>
        <label>Category<select><option>No category</option></select></label>
        <label class="product_description_field">Description<textarea rows="4"></textarea></label>
        </div></section></div><aside class="product_editor_side"><section class="product_editor_section">
        <h3>Product image</h3><div class="product_editor_preview">No image</div></section></aside></div>
        <div class="customers_actions product_editor_footer"><button>Cancel</button><button class="customer_primary">Save Item</button></div>
        </section></div>`);
      await page.addStyleTag({ content: fs.readFileSync(path.join(__dirname, '../crates/pos_desktop/assets/app.css'), 'utf8') });
      const input = page.locator('textarea');
      await input.fill('Product description\nSecond line');
      assert.equal(await input.inputValue(), 'Product description\nSecond line');
      const bounds = await input.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= width && bounds.height >= 100);
      const footer = await page.locator('.product_editor_footer').boundingBox();
      assert(bounds.y + bounds.height <= footer.y);
      await page.screenshot({ path: path.join(__dirname, `../target/product-description-${width}.png`) });
    }
    console.log('PASS: description multiline input and desktop/mobile layout');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
