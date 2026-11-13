// Drives the built app through WebDriver, against a real engine. Linux only:
// it uses WebKitGTK's own driver, which is what `tauri-driver` wraps.
//
//     npm run tauri -- build --debug --no-bundle
//     PHOTOS_TEST_BASE_URL=http://127.0.0.1:8098/api PHOTOS_TEST_API_KEY=... node scripts/e2e.mjs
//
// The app runs with its own config, data and cache folders (under a
// temporary folder), so your settings are not touched. The keychain is the
// real one: the script refuses to run when the app already has a key there,
// and removes the test key before it ends. Screenshots go to
// docs/screenshots/ with `--screenshots`.
//
// Native dialogs and drops from the file manager cannot be driven this way,
// so files reach the app through its own commands (the same ones the
// dialogs and drops call), followed by a reload.

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const app = process.env.PHOTOS_APP ?? join(root, "src-tauri/target/debug/photos-desktop");
const driverBinary = process.env.WEBKIT_WEBDRIVER ?? "/usr/bin/WebKitWebDriver";
const baseUrl = process.env.PHOTOS_TEST_BASE_URL;
const apiKey = process.env.PHOTOS_TEST_API_KEY;
const screenshots = process.argv.includes("--screenshots");
const port = 4445;

if (!baseUrl || !apiKey) {
  console.error("Set PHOTOS_TEST_BASE_URL and PHOTOS_TEST_API_KEY.");
  process.exit(2);
}
if (baseUrl.includes("photos.sh")) {
  console.error("This is not to be run against production.");
  process.exit(2);
}
for (const [what, path] of [["the app (build it first)", app], ["WebKitWebDriver", driverBinary]]) {
  if (!existsSync(path)) {
    console.error(`Missing ${what}: ${path}`);
    process.exit(2);
  }
}

// --- a tiny PNG writer, for photos to work on ---

function crc32(bytes) {
  let crc = ~0;
  for (const byte of bytes) {
    crc ^= byte;
    for (let k = 0; k < 8; k++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return ~crc >>> 0;
}

function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type), data]);
  const out = Buffer.alloc(body.length + 8);
  out.writeUInt32BE(data.length, 0);
  body.copy(out, 4);
  out.writeUInt32BE(crc32(body), body.length + 4);
  return out;
}

/** A product-like photo: a disc on a light backdrop, with a small blue speck. */
function photo(width, height, [r, g, b]) {
  const raw = Buffer.alloc((width * 3 + 1) * height);
  const radius = Math.min(width, height) * 0.3;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const at = y * (width * 3 + 1) + 1 + x * 3;
      const inDisc = Math.hypot(x - width / 2, y - height / 2) < radius;
      const inSpeck = Math.hypot(x - width * 0.85, y - height * 0.15) < radius * 0.18;
      const shade = 226 + ((x + y) % 10);
      const [pr, pg, pb] = inDisc ? [r, g, b] : inSpeck ? [40, 70, 200] : [shade, shade, shade];
      raw[at] = pr;
      raw[at + 1] = pg;
      raw[at + 2] = pb;
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// --- WebDriver ---

const wait = (ms) => new Promise((done) => setTimeout(done, ms));
let session = null;

async function call(method, path, body) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  const json = await response.json();
  if (!response.ok) throw new Error(`${method} ${path}: ${JSON.stringify(json.value)}`);
  return json.value;
}

const wd = (method, path, body) => call(method, `/session/${session}${path}`, body);

/** Runs a script in the page; the script calls `done(value)`. */
function run(script, ...args) {
  return wd("POST", "/execute/async", {
    script: `const done = arguments[arguments.length - 1]; (async () => { ${script} })().then(done, (e) => done({ __error: e && e.message ? e.message : JSON.stringify(e) }));`,
    args,
  }).then((value) => {
    if (value && value.__error) throw new Error(`in the page: ${value.__error}`);
    return value;
  });
}

/** Calls one of the app's commands the way the UI does. */
const invoke = (command, args = {}) =>
  run("return window.__TAURI_INTERNALS__.invoke(arguments[0], arguments[1]);", command, args);

async function find(using, value) {
  const element = await wd("POST", "/element", { using, value });
  return Object.values(element)[0];
}

const css = (selector) => find("css selector", selector);
/** A visible button by its text. */
const button = (text) =>
  find("xpath", `//button[not(ancestor::*[@hidden]) and starts-with(normalize-space(.), ${JSON.stringify(text)})]`);
const click = async (element) => wd("POST", `/element/${await element}/click`, {});
const type = async (element, text) => wd("POST", `/element/${await element}/value`, { text });
const clear = async (element) => wd("POST", `/element/${await element}/clear`, {});
const text = (selector) =>
  run("const e = document.querySelector(arguments[0]); return e ? e.innerText : null;", selector);

async function until(what, check, timeout = 90_000) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    try {
      last = await check();
      if (last) return last;
    } catch (e) {
      last = e.message;
    }
    await wait(250);
  }
  throw new Error(`timed out waiting for ${what}; last: ${JSON.stringify(last)}`);
}

async function choose(selector, value) {
  await run(
    "const s = document.querySelector(arguments[0]); s.value = arguments[1]; s.dispatchEvent(new Event('change', { bubbles: true }));",
    selector,
    value,
  );
}

async function shot(name) {
  if (!screenshots) return;
  const dir = join(root, "docs/screenshots");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `${name}.png`), Buffer.from(await wd("GET", "/screenshot"), "base64"));
  console.log(`  screenshot: docs/screenshots/${name}.png`);
}

/** Collects what the content security policy blocks (again after a reload). */
const violations = [];
async function watchViolations() {
  violations.push(...((await run("return window.__violations || [];")) ?? []));
  await run(`
    window.__violations = [];
    document.addEventListener("securitypolicyviolation", (e) => window.__violations.push(e.violatedDirective + " " + e.blockedURI));
  `);
}

async function reload() {
  await watchViolations();
  await wd("POST", "/refresh", {});
  await until("the app again", () => text(".rail"));
  await watchViolations();
}

const checks = [];
function ok(condition, what) {
  checks.push([Boolean(condition), what]);
  console.log(`${condition ? "  ok  " : "  FAIL"} ${what}`);
}

// --- the run ---

const work = mkdtempSync(join(tmpdir(), "photos-desktop-e2e-"));
const shop = join(work, "shop");
mkdirSync(join(shop, "more"), { recursive: true });
writeFileSync(join(shop, "mug.png"), photo(320, 240, [170, 30, 40]));
writeFileSync(join(shop, "vase.png"), photo(240, 320, [30, 110, 90]));
writeFileSync(join(shop, "more", "mug.png"), photo(200, 200, [200, 150, 20]));
writeFileSync(join(shop, "broken.png"), "this is not a picture");
writeFileSync(join(shop, "notes.txt"), "not a photo");
const bottle = join(work, "bottle.png");
writeFileSync(bottle, photo(384, 288, [150, 60, 20]));

// A stand-in for the API that holds photo requests open until released.
const slow = await (async () => {
  const waiting = [];
  let holding = true;
  let closed = 0;
  let answered = 0;
  const picture = photo(64, 64, [20, 120, 60]);
  const answer = (response) => {
    answered += 1;
    response.writeHead(200, { "content-type": "image/png", "x-image-width": "64", "x-image-height": "64" });
    response.end(picture);
  };
  const server = createServer((request, response) => {
    if (request.method === "GET") {
      const body = request.url.endsWith("/account") ? { account: "stand-in", unlimited: true } : [];
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(body));
      return;
    }
    request.resume();
    request.on("end", () => {
      if (!holding) return answer(response);
      waiting.push(response);
      response.on("close", () => {
        if (!response.writableEnded) closed += 1;
      });
    });
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  return {
    url: `http://127.0.0.1:${server.address().port}/api`,
    held: () => waiting.length,
    closed: () => closed,
    answered: () => answered,
    release: () => {
      holding = false;
    },
    stop: () => server.close(),
  };
})();

const driver = spawn(driverBinary, [`--port=${port}`, "--host=127.0.0.1"], {
  stdio: ["ignore", "ignore", "inherit"],
  env: {
    ...process.env,
    TAURI_WEBVIEW_AUTOMATION: "true",
    XDG_CONFIG_HOME: join(work, "config"),
    XDG_DATA_HOME: join(work, "data"),
    XDG_CACHE_HOME: join(work, "cache"),
  },
});

let failed = false;
let keyStored = false;
try {
  await until("the driver", () => call("GET", "/status").then((s) => s.ready), 15_000);
  const created = await call("POST", "/session", {
    capabilities: { alwaysMatch: { "webkitgtk:browserOptions": { binary: app, args: [] } } },
  });
  session = created.sessionId;
  await wd("POST", "/timeouts", { script: 120_000 });
  const origin = await until("the app's window", () => run("return location.origin;"), 30_000);
  if (!origin.startsWith("tauri://")) {
    // `cargo build` and `cargo test` leave a development binary in the same
    // place, which loads the dev server instead of the built UI.
    throw new Error(`the app shows ${origin}, not its built UI: build it again with "npm run tauri -- build --debug --no-bundle"`);
  }
  await until("the UI", () => text(".rail"), 30_000);
  await wd("POST", "/window/rect", { width: 1240, height: 840 }).catch(() => {});

  console.log("Start");
  await watchViolations();
  const status = await invoke("key_status");
  if (status.set) {
    throw new Error("the app already has an API key in the keychain: not touching it. Remove it in Settings to run this.");
  }
  ok(status.keychain_available, "the system keychain is available");
  ok((await text(".rail__account")).includes("Add your API key"), "without a key, the rail asks for one");
  ok(await run("await document.fonts.ready; return document.fonts.check('16px Geist');"), "the bundled font is loaded");

  console.log("Settings");
  await click(button("Settings"));
  await run("document.querySelector('details').open = true;");
  await clear(css("#base-url"));
  await type(css("#base-url"), baseUrl);
  await click(button("Save preferences"));
  await until("the settings to be saved", async () => (await invoke("settings_get")).base_url === baseUrl);
  ok(
    readFileSync(join(work, "config/sh.photos.desktop/settings.json"), "utf8").includes(baseUrl),
    "the base URL is in the settings file",
  );

  // A wrong key first: stored, tried, and reported in plain words.
  await type(css("#key-input"), "not-a-real-key-000000");
  keyStored = true;
  await click(button("Save and test"));
  const refused = await until("the refusal", async () => {
    const t = await text(".notice.tone-danger");
    return t && t.includes("not accepted") ? t : null;
  });
  ok(refused.includes("The API key was not accepted"), "a refused key is explained");
  await shot("settings-refused-key");

  await type(css("#key-input"), apiKey);
  await click(button("Save and test"));
  const works = await until("the key test", async () => {
    const t = await text(".notice.tone-success");
    return t && t.includes("The key works") ? t : null;
  });
  ok(true, `the key is accepted: "${works.trim()}"`);
  const stored = await invoke("key_status");
  ok(stored.storage === "keychain" && stored.last4 === apiKey.slice(-4), "the key is in the keychain; the UI knows only its end");
  ok((await run("return document.querySelector('#key-input').value;")) === "", "the pasted key is gone from the field");
  ok(!JSON.stringify(stored).includes(apiKey), "the key status does not hold the key");
  ok(
    !readdirSync(join(work, "config/sh.photos.desktop")).some((name) =>
      readFileSync(join(work, "config/sh.photos.desktop", name), "utf8").includes(apiKey),
    ),
    "the key is in no file of the app's config folder",
  );
  await run("document.querySelector('.scroll').scrollTop = 0;");
  await shot("settings");

  console.log("Batch");
  await click(button("Batch"));
  ok((await text(".empty h2")).includes("Drop photos or folders here"), "the empty batch says what to do");
  await shot("batch-empty");
  // What a drop of the folder does: the same command, then the list is read again.
  const added = await invoke("batch_add", { paths: [shop] });
  ok(added.items.length === 4, `the folder was walked: ${added.items.map((i) => i.name).join(", ")}`);
  await reload();
  await until("the list", async () => (await run("return document.querySelectorAll('.item').length;")) === 4);
  const thumbs = await until("the thumbnails", async () => {
    const sizes = await run(
      "return [...document.querySelectorAll('.item .thumb img')].map((i) => i.complete && i.naturalWidth);",
    );
    return sizes.length === 3 && sizes.every((w) => w > 0) ? sizes : null;
  });
  ok(true, `thumbnails come through the preview protocol (${thumbs.join(", ")} px wide); the broken file has none`);

  await choose("#batch-kind", "resize");
  await type(css("#batch-width"), "96");
  await choose("#batch-format", "webp");
  const cost = await until("the cost", async () => {
    const t = await text(".cost");
    return t && t.includes("4 files") ? t : null;
  });
  ok(cost.includes("unlimited"), `the cost line: "${cost.trim()}"`);
  await shot("batch-ready");
  await click(button("Start"));
  const summary = await until("the batch to finish", async () => {
    const t = await text("footer.status .notice.tone-warning, footer.status .notice.tone-success");
    return t && t.includes("done") ? t : null;
  });
  ok(summary.includes("3 of 4 files done, 1 failed"), `the summary: "${summary.trim().replace(/\s+/g, " ")}"`);
  const out = join(shop, "photos-sh");
  const results = readdirSync(out).sort();
  ok(
    JSON.stringify(results) === JSON.stringify(["mug-2.webp", "mug.webp", "vase.webp"]),
    `results on disk: ${results.join(", ")}`,
  );
  ok(readFileSync(join(shop, "mug.png")).length > 0 && existsSync(join(shop, "more", "mug.png")), "the sources are still there");
  const failedLine = await text(".item__detail--error");
  ok(failedLine && failedLine.length > 20, `the failed file says why: "${failedLine}"`);
  ok(Boolean(await button("Retry failed (1)")), "retry failed is offered");
  await until("the result thumbnails", async () => {
    const sizes = await run("return [...document.querySelectorAll('.item .thumb img')].map((i) => i.naturalWidth);");
    return sizes.length === 3 && sizes.every((w) => w > 0);
  });
  await shot("batch");

  console.log("Editor");
  await click(button("Editor"));
  ok((await text(".pane:not([hidden]) .empty h2")).includes("Drop a photo here"), "the empty editor says what to do");
  await invoke("editor_open", { path: bottle });
  await reload();
  await click(button("Editor"));
  const original = await until("the photo", async () => {
    const width = await run("const i = document.querySelector('.compare img'); return i && i.complete ? i.naturalWidth : 0;");
    return width > 0 ? width : null;
  });
  ok(original === 384, `the original is shown through the preview protocol (${original} px wide)`);

  // One model call: the background goes.
  await click(button("Apply"));
  ok((await until("the busy state", () => text(".working"))).includes("Removing the background"), "a running tool says so");
  await until("the cutout", async () => (await text(".steps")).includes("Remove background"), 120_000);
  const compare = await run(
    "return [...document.querySelectorAll('.compare img')].map((i) => i.alt + ' ' + i.naturalWidth).concat(document.querySelector('.slider') ? ['slider'] : []);",
  );
  ok(compare.length === 3 && compare.includes("slider"), `before/after: ${compare.join(" | ")}`);
  // The slider, by keyboard: the divide between before and after moves.
  await type(css(".slider"), "\uE014\uE014\uE014\uE014\uE014\uE014\uE014\uE014\uE014\uE014\uE014\uE014");
  const divide = await until("the slider", async () => {
    const left = await run("return document.querySelector('.divider').style.left;");
    return left !== "50%" ? left : null;
  });
  ok(divide === "62%", `the arrow keys move the before/after divide (to ${divide})`);
  await shot("editor");

  // Erase with the brush (one more model call): paint over the speck.
  await click(button("Erase"));
  await until("the canvas", () => run("return Boolean(document.querySelector('canvas'));"));
  await run(`
    const c = document.querySelector("canvas");
    const b = c.getBoundingClientRect();
    const at = (type, x, y) => c.dispatchEvent(new PointerEvent(type, { clientX: b.left + b.width * x, clientY: b.top + b.height * y, button: 0, pointerId: 1, bubbles: true }));
    at("pointerdown", 0.8, 0.1); at("pointermove", 0.85, 0.15); at("pointermove", 0.9, 0.2); at("pointerup", 0.9, 0.2);
  `);
  await shot("editor-erase");
  await click(find("xpath", "//button[contains(@class,'button--primary') and normalize-space(.)='Erase']"));
  await until("the erased step", async () => (await text(".steps")).includes("Erase"), 120_000);
  let snapshot = await invoke("editor_snapshot");
  ok(snapshot.cursor === 2 && snapshot.steps[1].label === "Erase", "erase made the second step");

  // Check (quality only: no model), then undo with the keyboard.
  await click(find("xpath", "//button[contains(@class,'tool') and normalize-space(.)='Check']"));
  await click(find("xpath", "//button[contains(@class,'button--primary') and normalize-space(.)='Check']"));
  const report = await until("the report", () => text(".report"));
  ok(report.startsWith("Check"), `the check reports: "${report.replace(/\s+/g, " ").slice(0, 90)}…"`);
  await run("window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true }));");
  await until("the undo", async () => (await invoke("editor_snapshot")).cursor === 1);
  ok((await text(".steps")).includes("Erase"), "undo keeps the step for redo");

  // Save as cannot be clicked through (a native dialog): the command behind it.
  ok(
    (await run("try { await window.__TAURI_INTERNALS__.invoke('editor_save', { path: arguments[0], replaceOriginal: false }); return 'saved'; } catch (e) { return e.code; }", bottle)) ===
      "confirm_replace_original",
    "saving over the original is refused until confirmed",
  );
  const saved = await invoke("editor_save", { path: join(work, "bottle-cutout"), replaceOriginal: false });
  ok(saved.endsWith("bottle-cutout.png") && existsSync(saved), `saved as ${saved}`);
  snapshot = await invoke("editor_snapshot");
  await click(button("Close"));
  await until("the editor to close", async () => (await invoke("editor_snapshot")) === null);

  console.log("The web view's limits");
  const denied = await run(
    "try { await window.__TAURI_INTERNALS__.invoke('plugin:dialog|open', { options: {} }); return 'allowed'; } catch (e) { return String(e); }",
  );
  ok(denied !== "allowed", `the web view cannot open dialogs itself: "${denied.slice(0, 70)}"`);
  const outside = await run(
    "try { const r = await fetch('https://example.com/', { mode: 'no-cors' }); return 'fetched ' + r.type; } catch (e) { return 'blocked'; }",
  );
  ok(outside === "blocked", "the web view cannot make outside requests (CSP)");
  const passwd = await run(
    "return await new Promise((r) => { const i = new Image(); i.onload = () => r('loaded'); i.onerror = () => r('refused'); i.src = 'preview://localhost/' + encodeURIComponent('/etc/passwd'); });",
  );
  ok(passwd === "refused", "the preview protocol serves no arbitrary path");
  await watchViolations();
  ok(
    violations.every((v) => v.includes("example.com")),
    `no CSP violations from the app itself (${violations.length} seen, from the probe above)`,
  );

  console.log("Cancel");
  // A stand-in server that holds every photo's answer open, to cancel under.
  await click(button("Settings"));
  await run("document.querySelector('.scroll').scrollTop = 0; document.querySelector('details').open = true;");
  await clear(css("#base-url"));
  await type(css("#base-url"), slow.url);
  await click(button("Save preferences"));
  await until("the stand-in server to be used", async () => (await invoke("settings_get")).base_url === slow.url);
  await click(button("Batch"));
  await click(button("Clear list"));
  await invoke("batch_add", { paths: [shop] });
  await reload();
  await until("the list", async () => (await run("return document.querySelectorAll('.item').length;")) === 4);
  const before = readdirSync(out).length;
  await click(button("Start"));
  await until("three photos in flight", () => slow.held() === 3, 30_000);
  ok((await text(".panel__foot")).includes("may still cost its credits"), "the cancel button says what cancelling may cost");
  const cancelAt = Date.now();
  await click(button("Cancel"));
  const cancelled = await until("the cancelled run to end", async () => {
    const t = await text("footer.status .notice");
    return t && t.includes("Cancelled.") ? t : null;
  }, 15_000);
  ok(Date.now() - cancelAt < 5_000, `cancel ends the run at once (${Date.now() - cancelAt} ms, with the answers still held open)`);
  ok(
    cancelled.includes("0 of 4 files done, 3 cancelled while in progress, 1 not run") && cancelled.includes("normally not charged"),
    `the summary: "${cancelled.trim().replace(/\s+/g, " ")}"`,
  );
  const pills = await run("return [...document.querySelectorAll('.item .pill')].map((p) => p.innerText.trim());");
  ok(
    pills.filter((p) => p === "Cancelled").length === 3 && pills.filter((p) => p === "Not run").length === 1,
    `statuses: ${pills.join(", ")}`,
  );
  await until("the server to see its clients go", () => slow.closed() === 3, 10_000);
  ok(slow.closed() === 3 && slow.answered() === 0, "the three requests were dropped: the server saw the connections close");
  ok(readdirSync(out).length === before && !readdirSync(out).some((n) => n.includes(".part")), "nothing was written for the cancelled photos");
  slow.release();
  await click(button("Retry cancelled and not run (4)"));
  const retried = await until("the retry to finish", async () => {
    const t = await text("footer.status .notice.tone-success");
    return t && t.includes("All 4 files done") ? t : null;
  }, 30_000);
  ok(Boolean(retried) && readdirSync(out).length === before + 4, "retry runs the cancelled and the not-run photos again");

  console.log("Remove the key");
  await click(button("Settings"));
  await click(button("Remove"));
  await until("the key to go", async () => !(await invoke("key_status")).set);
  keyStored = false;
  ok((await text(".rail__account")).includes("Add your API key"), "the key is removed");
} catch (error) {
  failed = true;
  console.error(`\nFAILED: ${error.message}`);
} finally {
  if (session && keyStored) {
    // Never leave the test key behind.
    await invoke("key_remove").catch((e) => console.error(`could not remove the key: ${e.message}`));
  }
  if (session) await call("DELETE", `/session/${session}`).catch(() => {});
  driver.kill();
  slow.stop();
}

const bad = checks.filter(([passed]) => !passed).length;
console.log(`\n${checks.length - bad} of ${checks.length} checks passed. Work folder: ${work}`);
process.exit(failed || bad > 0 ? 1 : 0);
