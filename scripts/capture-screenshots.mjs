import { chromium } from "playwright";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(__dirname, "../docs/screenshots");
const baseUrl = "http://127.0.0.1:4173";

async function capture() {
  await mkdir(outDir, { recursive: true });
  const browser = await chromium.launch();
  const page = await browser.newPage({
    viewport: { width: 1280, height: 820 },
    deviceScaleFactor: 2,
  });

  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(outDir, "chat.png"), fullPage: false });

  await page.locator("button.rail-btn[title='模型服务']").click();
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(outDir, "providers.png"), fullPage: false });

  await page.locator("button.rail-btn[title='设置']").click();
  await page.waitForTimeout(600);
  await page.locator("button.settings-nav-item", { hasText: "扩展" }).click();
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(outDir, "extensions.png"), fullPage: false });

  await page.locator("button.rail-btn[title='对话']").click();
  await page.waitForTimeout(400);
  await page.locator("button.rail-btn[title='导入记录']").click();
  await page.waitForTimeout(600);
  await page.screenshot({ path: path.join(outDir, "import.png"), fullPage: false });

  await browser.close();
  console.log("Screenshots saved to", outDir);
}

capture().catch((err) => {
  console.error(err);
  process.exit(1);
});
