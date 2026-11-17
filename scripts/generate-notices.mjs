import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { TextDecoder } from 'node:util';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const target = 'x86_64-pc-windows-msvc';
const args = process.argv.slice(2);
const option = (name) => {
  const index = args.indexOf(name);
  if (index < 0) return undefined;
  const value = args[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`${name} requires a path`);
  return path.resolve(value);
};
const output = option('--output') ?? path.join(root, 'src-tauri/resources/THIRD_PARTY_NOTICES.txt');
const compare = args.includes('--check');
const allowMissing = args.includes('--allow-missing');
const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
const sort = (left, right) => left < right ? -1 : left > right ? 1 : 0;
const runJson = (executable, parameters, cwd) => JSON.parse(execFileSync(executable, parameters, {
  cwd, encoding: 'utf8', windowsHide: true, maxBuffer: 64 * 1024 * 1024,
  stdio: ['ignore', 'pipe', 'pipe'],
}));

export function findPnpmEntry({
  pathValue = process.env.PATH ?? '', nodeExecutable = process.execPath,
  npmExecPath = process.env.npm_execpath, pnpmCliJs = process.env.PNPM_CLI_JS,
} = {}) {
  // Invoke JavaScript entry points directly: Windows .cmd files need a shell.
  const candidates = [pnpmCliJs, npmExecPath];
  for (const directory of [...pathValue.split(path.delimiter), path.dirname(nodeExecutable), root]) {
    if (!directory) continue;
    candidates.push(path.join(directory, 'pnpm.cjs'));
    // pnpm/action-setup exposes setup-pnpm/node_modules/.bin on Windows runners.
    candidates.push(path.join(directory, '../pnpm/bin/pnpm.cjs'));
    candidates.push(path.join(directory, 'node_modules/pnpm/bin/pnpm.cjs'));
    candidates.push(path.join(directory, 'node_modules/corepack/dist/pnpm.js'));
    if (process.platform !== 'win32') candidates.push(path.join(directory, '../lib/node_modules/pnpm/bin/pnpm.cjs'));
  }
  return candidates.find((candidate) => candidate && /(?:pnpm\.cjs|pnpm\.js)$/.test(candidate) && fs.existsSync(candidate));
}

function pnpmLicenses() {
  const supplied = option('--pnpm-json');
  if (supplied) return readJson(supplied);
  const cli = findPnpmEntry();
  if (!cli && process.platform === 'win32') throw new Error('Cannot find pnpm JavaScript entry. Set PNPM_CLI_JS or pass --pnpm-json.');
  const executable = cli ? process.execPath : 'pnpm';
  const prefix = cli ? [cli] : [];
  const actual = execFileSync(executable, [...prefix, '--version'], { cwd: root, encoding: 'utf8', windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  const expected = /^pnpm@([^+]+)/.exec(readJson(path.join(root, 'package.json')).packageManager)?.[1];
  if (!expected || actual !== expected) throw new Error(`License generation requires pnpm ${expected ?? 'declared in packageManager'}; found ${actual}`);
  return runJson(executable, [...prefix, 'licenses', 'list', '--prod', '--json'], root);
}

function cargoMetadata() {
  const supplied = option('--cargo-json');
  if (supplied) return readJson(supplied);
  const userCargo = path.join(process.env.CARGO_HOME ?? path.join(os.homedir(), '.cargo'), 'bin', process.platform === 'win32' ? 'cargo.exe' : 'cargo');
  const cargo = process.env.CARGO ?? (fs.existsSync(userCargo) ? userCargo : 'cargo');
  return runJson(cargo, ['metadata', '--locked', '--format-version', '1', '--filter-platform', target], path.join(root, 'src-tauri'));
}

function npmPackages(licenses) {
  const found = new Map();
  for (const [expression, packages] of Object.entries(licenses)) {
    if (!Array.isArray(packages)) throw new Error('Unexpected pnpm licenses JSON structure');
    for (const pkg of packages) {
      for (const directory of pkg.paths ?? []) {
        const manifest = readJson(path.join(directory, 'package.json'));
        const key = `npm:${manifest.name}@${manifest.version}`;
        if (found.has(key)) continue;
        found.set(key, {
          key, name: manifest.name, version: manifest.version, ecosystem: 'npm', directory,
          license: typeof manifest.license === 'string' ? manifest.license : pkg.license ?? expression,
          authors: [manifest.author ?? pkg.author].filter(Boolean).map((author) => typeof author === 'string' ? author : [author.name, author.email].filter(Boolean).join(' ')),
          source: manifest.homepage ?? pkg.homepage ?? '',
        });
      }
    }
  }
  return [...found.values()];
}

function rustPackages(metadata) {
  if (!metadata.resolve?.root) throw new Error('Cargo metadata has no root dependency graph');
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const reachable = new Set();
  const visit = (id) => {
    if (reachable.has(id)) return;
    reachable.add(id);
    for (const dependency of nodes.get(id)?.deps ?? []) {
      if (dependency.dep_kinds.some((kind) => kind.kind !== 'dev')) visit(dependency.pkg);
    }
  };
  visit(metadata.resolve.root);
  return metadata.packages.filter((pkg) => reachable.has(pkg.id) && pkg.id !== metadata.resolve.root).map((pkg) => ({
    key: `cargo:${pkg.name}@${pkg.version}`, ecosystem: 'cargo', name: pkg.name, version: pkg.version,
    directory: path.dirname(pkg.manifest_path), license: pkg.license ?? 'Not declared',
    licenseFile: pkg.license_file, authors: pkg.authors ?? [], source: pkg.repository ?? pkg.homepage ?? '',
  }));
}

function licenseTexts(pkg, omissions) {
  const files = new Set();
  const gather = (file) => {
    const stat = fs.lstatSync(file);
    if (stat.isSymbolicLink()) {
      const resolved = fs.realpathSync(file);
      if (path.relative(pkg.directory, resolved).startsWith('..')) {
        omissions.push(`${pkg.key}: license symlink leaves package directory (${path.basename(file)})`);
        return;
      }
    }
    if (fs.statSync(file).isDirectory()) {
      for (const child of fs.readdirSync(file).sort(sort)) gather(path.join(file, child));
    } else if (fs.statSync(file).isFile()) files.add(file);
  };
  for (const name of fs.readdirSync(pkg.directory).sort(sort)) {
    if (/^(licen[cs]e|copying|notice)/i.test(name)) gather(path.join(pkg.directory, name));
  }
  if (pkg.licenseFile) {
    const explicit = path.resolve(pkg.directory, pkg.licenseFile);
    if (!path.relative(pkg.directory, explicit).startsWith('..') && fs.existsSync(explicit)) gather(explicit);
    else omissions.push(`${pkg.key}: declared license_file is unavailable inside the package`);
  }
  const texts = [];
  for (const file of [...files].sort(sort)) {
    const label = path.relative(pkg.directory, file).split(path.sep).join('/');
    const bytes = fs.readFileSync(file);
    let text;
    try {
      if (bytes.includes(0)) throw new Error('binary file');
      text = new TextDecoder('utf-8', { fatal: true }).decode(bytes).replace(/\r\n?/g, '\n').trim();
      if (!text) throw new Error('empty file');
    } catch {
      omissions.push(`${pkg.key}: ${label} is not nonempty UTF-8 text`);
      continue;
    }
    texts.push({ label, text });
  }
  for (const supplement of pkg.supplement?.files ?? []) {
    const vendorDirectory = path.join(root, 'licenses/upstream');
    const file = path.resolve(vendorDirectory, supplement.file);
    const relative = path.relative(vendorDirectory, file);
    if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error(`Invalid vendored license path for ${pkg.key}`);
    if (!/^https:\/\/[^/]+\/.*\/[a-f0-9]{40}\//.test(supplement.sourceURL)) throw new Error(`Supplement source must name a fixed commit for ${pkg.key}`);
    const bytes = fs.readFileSync(file);
    if (createHash('sha256').update(bytes).digest('hex') !== supplement.sha256) throw new Error(`Vendored license SHA256 mismatch: ${supplement.file}`);
    const body = new TextDecoder('utf-8', { fatal: true }).decode(bytes).replace(/\r\n?/g, '\n').trim();
    if (!body || bytes.includes(0)) throw new Error(`Invalid vendored license text: ${supplement.file}`);
    texts.push({ label: `vendored/${supplement.file}`, text: [
      `Source: ${supplement.sourceURL}`, `Source file SHA256: ${supplement.sourceSha256 ?? supplement.sha256}`,
      ...(supplement.extraction ? [`Extraction: ${supplement.extraction}`] : []), '', body,
    ].join('\n') });
  }
  if (!texts.length) omissions.push(`${pkg.key}: no packaged or verified supplemental license text found (declared ${pkg.license})`);
  return texts;
}

function generate() {
  const packages = [...npmPackages(pnpmLicenses()), ...rustPackages(cargoMetadata())].sort((a, b) => sort(a.key, b.key));
  const overridesFile = path.join(root, 'licenses/overrides.json');
  const overrides = fs.existsSync(overridesFile) ? readJson(overridesFile) : { schemaVersion: 1, packages: {} };
  if (overrides.schemaVersion !== 1) throw new Error('Unsupported license overrides schema');
  const omissions = [];
  const sections = packages.map((pkg) => {
    pkg.supplement = overrides.packages[pkg.key];
    const texts = licenseTexts(pkg, omissions);
    return [
      '='.repeat(78), `${pkg.ecosystem}: ${pkg.name} ${pkg.version}`, `Declared license: ${pkg.license}`,
      ...(pkg.authors.length ? [`Authors (package metadata): ${pkg.authors.join('; ')}`] : []),
      ...(/^https?:\/\//.test(pkg.source) ? [`Upstream: ${pkg.source}`] : []),
      ...(pkg.supplement ? [`Supplement provenance: ${pkg.supplement.note}`, ...pkg.supplement.evidence.map((url) => `Evidence: ${url}`)] : []),
      '', ...texts.flatMap(({ label, text }) => [`--- ${label} ---`, text, '']),
      ...(!texts.length ? ['LICENSE TEXT MISSING: see the omissions report above.', ''] : []),
    ].join('\n');
  });
  const uniqueOmissions = [...new Set(omissions)].sort(sort);
  const content = [
    'CLAUDE CODE MANAGER — THIRD-PARTY LICENSES AND NOTICES',
    '', 'Generated from installed, locked dependency packages by scripts/generate-notices.mjs.',
    'Scope: pnpm production dependency graph and Cargo non-development graph for',
    `${target}, including Rust build dependencies.`,
    'Package metadata identifies authors; original copyright notices are retained in the texts below.',
    'Missing package files may be supplemented by hash-verified, commit-pinned sources.',
    'Standard license texts and source copyright excerpts are explicitly identified where used.',
    'No license text or copyright attribution is invented.',
    `Packages: ${packages.length} (npm: ${packages.filter((pkg) => pkg.ecosystem === 'npm').length}; Cargo: ${packages.filter((pkg) => pkg.ecosystem === 'cargo').length}).`,
    '', 'OMISSIONS REPORT', ...(uniqueOmissions.length ? uniqueOmissions.map((item) => `- ${item}`) : ['None.']),
    '', ...sections, '',
  ].join('\n');
  for (const privatePath of [root, os.homedir()]) {
    if (content.includes(privatePath) || content.includes(privatePath.split(path.sep).join('/'))) throw new Error('Generated notice unexpectedly contains a local absolute path');
  }
  if (compare) {
    if (!fs.existsSync(output) || fs.readFileSync(output, 'utf8') !== content) throw new Error('Third-party notices are stale. Run node scripts/generate-notices.mjs.');
  } else {
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, content, 'utf8');
  }
  console.log(`${compare ? 'Verified' : 'Generated'} ${packages.length} package notices; ${uniqueOmissions.length} omission(s). SHA256 ${createHash('sha256').update(content).digest('hex')}`);
  if (uniqueOmissions.length) {
    console.error(uniqueOmissions.join('\n'));
    if (!allowMissing) process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { generate(); }
  catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
