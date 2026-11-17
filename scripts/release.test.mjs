import assert from 'node:assert/strict'
import test from 'node:test'
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { prepareRelease, updaterManifest, installerFileName, repository, sha256 } from './release.mjs'
import { publishRelease, verifyAsset } from './publish-release.mjs'

const version = '0.1.0-beta.1'
const tag = `v${version}`
const sourceName = `Claude Code Manager_${version}_x64-setup.exe`
const safeName = `claude-code-manager_${version}_x64-setup.exe`
const signature = Buffer.from(`untrusted comment: fixture\nfixture\ntrusted comment: timestamp:123 file:${sourceName}\nfixture\n`).toString('base64')
const assetUrl = (name) => `https://github.com/${repository}/releases/download/${tag}/${name}`
const json = (value) => ({ ok: true, json: async () => value })
const draft = (assets = []) => ({ tag_name: tag, draft: true, assets, upload_url: `https://uploads.github.com/repos/${repository}/releases/123/assets{?name,label}`, html_url: `https://github.com/${repository}/releases/tag/${tag}` })

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'ccm-release-'))
  t.after(async () => {
    assert.ok(root.startsWith(join(tmpdir(), 'ccm-release-')))
    await rm(root, { recursive: true, force: true })
  })
  const bundle = join(root, 'src-tauri/target/release/bundle/nsis')
  await mkdir(bundle, { recursive: true })
  await writeFile(join(root, 'package.json'), JSON.stringify({ version }))
  await writeFile(join(bundle, sourceName), 'installer-fixture')
  await writeFile(join(bundle, `${sourceName}.sig`), signature)
  return { root, bundle }
}

async function preparedFixture(t) {
  const result = await fixture(t)
  return { ...result, ...await prepareRelease({ root: result.root, tag, commit: 'a'.repeat(40), date: '2026-01-01T00:00:00Z' }) }
}

test('updater URL uses only the canonical ASCII installer name for the exact version', () => {
  assert.equal(installerFileName(version), safeName)
  const manifest = updaterManifest(version, safeName, signature, '2026-01-01T00:00:00Z')
  assert.equal(manifest.platforms['windows-x86_64'].url, assetUrl(safeName))
  assert.throws(() => updaterManifest(version, sourceName, signature, ''), /filename/)
  assert.throws(() => updaterManifest('0.1.0', safeName, signature, ''), /filename/)
  assert.throws(() => updaterManifest(version, '../wrong_x64-setup.exe', signature, ''), /filename/)
  assert.throws(() => updaterManifest(version, safeName, '', ''), /signature/)
})

test('release preparation maps a spaced source name to consistent installer, signature, URL and SHA256 names', async (t) => {
  const { root, bundle } = await fixture(t)
  await assert.rejects(() => prepareRelease({ root, tag: 'v0.2.0', commit: 'a'.repeat(40) }), /Tag/)
  const result = await prepareRelease({ root, tag, commit: 'a'.repeat(40), date: '2026-01-01T00:00:00Z' })
  assert.deepEqual((await readdir(result.output)).sort(), [safeName, `${safeName}.sig`, 'latest.json', 'provenance.json', 'SHA256SUMS'].sort())
  assert.deepEqual(await readFile(join(result.output, safeName)), await readFile(join(bundle, sourceName)))
  assert.deepEqual(await readFile(join(result.output, `${safeName}.sig`)), await readFile(join(bundle, `${sourceName}.sig`)), 'signature bytes and original trusted comment must remain untouched')
  const checksums = (await readFile(join(result.output, 'SHA256SUMS'), 'utf8')).trim().split('\n')
  assert.equal(checksums.length, 4)
  for (const line of checksums) {
    const name = line.slice(66)
    assert.ok(result.files.includes(name))
    assert.equal(line.slice(0, 64), sha256(await readFile(join(result.output, name))))
    assert.doesNotMatch(name, /\s/)
  }
  const metadata = JSON.parse(await readFile(join(result.output, 'latest.json'), 'utf8'))
  assert.equal(metadata.platforms['windows-x86_64'].signature, signature)
  assert.equal(metadata.platforms['windows-x86_64'].url, assetUrl(safeName))
  assert.ok(checksums.some((line) => line.endsWith(`  ${safeName}`)))
  const provenance = JSON.parse(await readFile(join(result.output, 'provenance.json'), 'utf8'))
  assert.equal(provenance.tag, tag)
  assert.equal(provenance.commit, 'a'.repeat(40))
})

test('asset verification rejects GitHub renames and mismatched URLs even when bytes match', () => {
  const digest = sha256('installer-fixture')
  const asset = { name: safeName, browser_download_url: assetUrl(safeName), digest: `sha256:${digest}` }
  verifyAsset(asset, { name: safeName, digest, tag })
  assert.throws(() => verifyAsset({ ...asset, name: safeName.replaceAll('-', '.') }, { name: safeName, digest, tag }), /renamed/)
  assert.throws(() => verifyAsset({ ...asset, browser_download_url: assetUrl(sourceName) }, { name: safeName, digest, tag }), /URL mismatch/)
  assert.throws(() => verifyAsset({ ...asset, browser_download_url: `https://example.com/${safeName}` }, { name: safeName, digest, tag }), /URL mismatch/)
})

test('publishing stops immediately if the upload response renames an asset', async (t) => {
  const { root } = await preparedFixture(t)
  let uploads = 0
  const fetchImpl = async (url, options = {}) => {
    if (!options.method) return json([draft()])
    uploads++
    const name = new URL(url).searchParams.get('name')
    return json({ name: `${name}.renamed`, browser_download_url: assetUrl(`${name}.renamed`), digest: `sha256:${sha256(options.body)}` })
  }
  await assert.rejects(() => publishRelease({ root, tag, token: 'fixture-token', fetchImpl }), /renamed/)
  assert.equal(uploads, 1)
})

test('same-name assets with different or unknown digests are never overwritten', async (t) => {
  const { root } = await preparedFixture(t)
  for (const digest of [undefined, `sha256:${sha256('different installer')}`]) {
    let uploads = 0
    const fetchImpl = async (_url, options = {}) => {
      if (options.method) { uploads++; throw new Error('Must never upload over an existing asset') }
      return json([draft([{ name: safeName, browser_download_url: assetUrl(safeName), digest }])])
    }
    await assert.rejects(() => publishRelease({ root, tag, token: 'fixture-token', fetchImpl }), /Refusing to replace existing asset/)
    assert.equal(uploads, 0)
  }
})

test('an unchanged draft rerun verifies existing names, URLs and digests without uploading', async (t) => {
  const { root, output, files } = await preparedFixture(t)
  const assets = await Promise.all(files.map(async (name) => ({ name, browser_download_url: assetUrl(name), digest: `sha256:${sha256(await readFile(join(output, name)))}` })))
  let calls = 0
  const fetchImpl = async (_url, options = {}) => {
    calls++
    assert.equal(options.method, undefined)
    return json([draft(assets)])
  }
  await publishRelease({ root, tag, token: 'fixture-token', fetchImpl })
  assert.equal(calls, 1)
})
