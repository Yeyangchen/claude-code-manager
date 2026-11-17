import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { findPnpmEntry } from './generate-notices.mjs';

const generator = fileURLToPath(new URL('./generate-notices.mjs', import.meta.url));

function fixture(t, withLicense = true) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ccm-notices-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const npmRoot = path.join(directory, 'npm-package');
  const cargoRoot = path.join(directory, 'rust-package');
  fs.mkdirSync(npmRoot);
  fs.mkdirSync(cargoRoot);
  fs.writeFileSync(path.join(npmRoot, 'package.json'), JSON.stringify({ name: 'license-fixture', version: '1.2.3', license: 'MIT', author: 'Fixture author' }));
  if (withLicense) fs.writeFileSync(path.join(npmRoot, 'LICENSE'), 'Copyright Fixture author\r\nActual package license text.\r\n');
  fs.writeFileSync(path.join(cargoRoot, 'legal.txt'), 'Copyright Rust fixture author\nExplicit license_file text.\n');
  const pnpm = path.join(directory, 'pnpm.json');
  const cargo = path.join(directory, 'cargo.json');
  const output = path.join(directory, 'notices.txt');
  fs.writeFileSync(pnpm, JSON.stringify({ MIT: [{ name: 'license-fixture', versions: ['1.2.3'], paths: [npmRoot] }] }));
  fs.writeFileSync(cargo, JSON.stringify({
    resolve: { root: 'app', nodes: [
      { id: 'app', deps: [{ pkg: 'rust', dep_kinds: [{ kind: null }] }, { pkg: 'dev', dep_kinds: [{ kind: 'dev' }] }] },
      { id: 'rust', deps: [] },
    ] },
    packages: [
      { id: 'app', name: 'app', version: '1.0.0', manifest_path: path.join(directory, 'Cargo.toml') },
      { id: 'rust', name: 'rust-license-fixture', version: '2.0.0', manifest_path: path.join(cargoRoot, 'Cargo.toml'), license: 'MIT', license_file: 'legal.txt', authors: ['Rust fixture author'] },
      { id: 'dev', name: 'dev-only-fixture', version: '3.0.0', manifest_path: '/must-not-read/Cargo.toml' },
    ],
  }));
  const run = (...extra) => spawnSync(process.execPath, [generator, '--pnpm-json', pnpm, '--cargo-json', cargo, '--output', output, ...extra], { encoding: 'utf8', windowsHide: true });
  return { directory, output, run };
}

test('aggregates actual texts, respects license_file, excludes dev-only graph, and checks deterministically', (t) => {
  const { directory, output, run } = fixture(t);
  const generated = run();
  assert.equal(generated.status, 0, generated.stderr);
  const content = fs.readFileSync(output, 'utf8');
  assert.match(content, /npm: license-fixture 1\.2\.3/);
  assert.match(content, /Copyright Fixture author\nActual package license text\./);
  assert.match(content, /--- legal\.txt ---\nCopyright Rust fixture author/);
  assert.doesNotMatch(content, /dev-only-fixture|must-not-read/);
  assert.equal(content.includes(directory), false);
  assert.equal(content.includes('\r'), false);
  assert.equal(run('--check').status, 0);
  fs.appendFileSync(output, 'stale output');
  const stale = run('--check');
  assert.equal(stale.status, 1);
  assert.match(stale.stderr, /notices are stale/);
  assert.ok(fs.readFileSync(output, 'utf8').endsWith('stale output'), '--check must never overwrite');
});

test('reports missing license text and fails rather than manufacturing a standard license', (t) => {
  const { output, run } = fixture(t, false);
  const generated = run();
  assert.equal(generated.status, 1);
  assert.match(generated.stderr, /license-fixture@1\.2\.3: no packaged or verified supplemental license text/);
  const content = fs.readFileSync(output, 'utf8');
  assert.match(content, /LICENSE TEXT MISSING/);
  assert.doesNotMatch(content, /Permission is hereby granted/);
});

test('finds the pnpm/action-setup Windows directory layout without relying on Corepack', (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ccm-pnpm-layout-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const bin = path.join(directory, 'setup-pnpm/node_modules/.bin');
  const entry = path.join(directory, 'setup-pnpm/node_modules/pnpm/bin/pnpm.cjs');
  fs.mkdirSync(bin, { recursive: true });
  fs.mkdirSync(path.dirname(entry), { recursive: true });
  fs.writeFileSync(entry, '// fixture');
  assert.equal(findPnpmEntry({ pathValue: bin, nodeExecutable: path.join(directory, 'node.exe'), npmExecPath: '', pnpmCliJs: '' }), entry);
});

test('rejects a package-manager version that differs from the project pin', (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'ccm-pnpm-version-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const entry = path.join(directory, 'pnpm.cjs');
  fs.writeFileSync(entry, "process.stdout.write('1.0.0\\n');\n");
  const result = spawnSync(process.execPath, [generator, '--check'], { encoding: 'utf8', windowsHide: true, env: { ...process.env, PNPM_CLI_JS: entry } });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /requires pnpm 11\.7\.0; found 1\.0\.0/);
});
