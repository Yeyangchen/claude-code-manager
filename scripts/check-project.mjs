import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauri = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const cargo = await readFile('src-tauri/Cargo.toml', 'utf8');
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
assert.equal(tauri.version, packageJson.version, 'Tauri version differs from package.json');
assert.equal(cargoVersion, packageJson.version, 'Cargo version differs from package.json');
assert.equal(tauri.identifier, 'io.github.yeyangchen.claude-code-manager');
assert.match(packageJson.packageManager, /^pnpm@\d+\.\d+\.\d+$/);
assert.match(tauri.app.security.csp, /default-src 'self'/);
assert.doesNotMatch(tauri.app.security.csp, /unsafe-eval/);
for (const file of ['README.md', 'LICENSE', 'CONTRIBUTING.md', 'SECURITY.md', 'PROJECT_PLAN.md']) {
  assert.ok((await readFile(file, 'utf8')).trim().length > 0, `${file} is empty`);
}
console.log(`Project metadata and version ${packageJson.version} verified.`);
