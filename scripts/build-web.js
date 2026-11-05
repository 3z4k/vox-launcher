// Type-checks the UI with tsc, bundles it with esbuild and assembles dist-web/, the folder Tauri embeds.
const fs = require('node:fs');
const path = require('node:path');
const { execSync } = require('node:child_process');
const esbuild = require('esbuild');

const root = path.join(__dirname, '..');
const src = path.join(root, 'src', 'renderer');
const out = path.join(root, 'dist-web');

execSync('npx tsc -p tsconfig.json --noEmit', { cwd: root, stdio: 'inherit' });
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });
esbuild.buildSync({ entryPoints: [path.join(src, 'main.ts')], bundle: true, format: 'iife', target: 'es2022', outfile: path.join(out, 'app.js'), minify: false, logLevel: 'warning' });
for (const f of ['index.html', 'styles.css']) fs.copyFileSync(path.join(src, f), path.join(out, f));
fs.copyFileSync(path.join(root, 'src-tauri', 'icons', '128x128.png'), path.join(out, 'favicon.png'));
console.log('Built dist-web');
