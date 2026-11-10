// Starts the playground server and a headless Chrome with the DevTools port open, for the other scripts in this folder.
//   node launch.mjs [serverPort=8766] [debugPort=9333]     keeps both running until Ctrl-C and prints the URL
// Other scripts import `launch()` and close it when they are done. Not part of CI.
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, dirname} from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

async function until(check, what, ms = 20000) {
  const start = Date.now();
  while (Date.now() - start < ms) {
    try { if (await check()) return; } catch { /* not yet */ }
    await new Promise(r => setTimeout(r, 200));
  }
  throw new Error(`${what} did not start in ${ms} ms`);
}

export async function launch({serverPort = 8766, debugPort = 9333} = {}) {
  const server = spawn('python3', [join(here, '..', 'server.py'), '--port', String(serverPort)], {stdio: 'ignore'});
  const chrome = spawn(CHROME, ['--headless=new', `--remote-debugging-port=${debugPort}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), 'lw-chrome-'))}`, '--window-size=1400,900', 'about:blank'], {stdio: 'ignore'});
  const close = () => { server.kill(); chrome.kill(); };
  try {
    await until(async () => (await fetch(`http://127.0.0.1:${serverPort}/`)).ok, 'the server');
    await until(async () => (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).ok, 'Chrome');
  } catch (error) { close(); throw error; }
  return {url: `http://127.0.0.1:${serverPort}/`, debugPort, close};
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [serverPort, debugPort] = process.argv.slice(2).map(Number);
  const running = await launch({serverPort: serverPort || undefined, debugPort: debugPort || undefined});
  console.log(`url=${running.url} debugPort=${running.debugPort}`);
  process.on('SIGINT', () => { running.close(); process.exit(0); });
}
