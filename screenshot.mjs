#!/usr/bin/env node
import http from 'http';
import { default as WebSocket } from 'ws';
import fs from 'fs';

const pageId = '8C33D9574813FA4FE73C2CF0362AB7E8';
const wsUrl = `ws://localhost:9222/devtools/page/${pageId}`;

async function main() {
  const ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {
    ws.on('open', resolve);
    ws.on('error', reject);
  });

  let msgId = 1;

  function send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = msgId++;
      const timeout = setTimeout(() => reject(new Error('timeout')), 10000);

      function handler(data) {
        const msg = JSON.parse(data.toString());
        if (msg.id === id) {
          clearTimeout(timeout);
          ws.off('message', handler);
          resolve(msg.result);
        }
      }
      ws.on('message', handler);
      ws.send(JSON.stringify({ id, method, params }));
    });
  }

  try {
    // Reload the page to get fresh content
    await send('Page.reload');
    await new Promise(r => setTimeout(r, 4000));

    // Take screenshot
    const result = await send('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync('/tmp/boards-current.png', Buffer.from(result.data, 'base64'));
    console.log('Screenshot saved: /tmp/boards-current.png');

    // Get page HTML
    const htmlResult = await send('Runtime.evaluate', {
      expression: 'document.querySelector(".soroban-render-view")?.innerHTML || "no content"',
      returnByValue: true
    });
    console.log('Content preview:', htmlResult.result?.value?.substring(0, 500));

    // Get all links
    const linksResult = await send('Runtime.evaluate', {
      expression: `JSON.stringify(Array.from(document.querySelectorAll('a[data-action]')).map(a => ({text: a.textContent, action: a.dataset.action})))`,
      returnByValue: true
    });
    console.log('Links:', linksResult.result?.value);

  } catch (e) {
    console.error('Error:', e.message);
  }

  ws.close();
}

main();
