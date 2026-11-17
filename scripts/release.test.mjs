import assert from 'node:assert/strict'
import test from 'node:test'
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { prepareRelease, updaterManifest, sha256 } from './release.mjs'

const signature = Buffer.from('untrusted comment: fixture\nfixture\ntrusted comment: fixture\nfixture\n').toString('base64')
test('updater URL remains versioned and rejects unintended installer targets', () => {
  const manifest = updaterManifest('0.1.0-beta.1', 'Claude Code Manager_0.1.0-beta.1_x64-setup.exe', signature, '2026-01-01T00:00:00Z')
  assert.match(manifest.platforms['windows-x86_64'].url, /download\/v0\.1\.0-beta\.1\/Claude%20Code%20Manager_/)
  assert.throws(() => updaterManifest('0.1.0', '../wrong_x64-setup.exe', signature, ''), /filename/)
  assert.throws(() => updaterManifest('0.1.0', 'test_x64-setup.exe', '', ''), /signature/)
})
test('release preparation binds the tag, commit, binary and checksums', async () => {
  const root = await mkdtemp(join(tmpdir(), 'ccm-release-'))
  try {
    const bundle = join(root, 'src-tauri/target/release/bundle/nsis')
    await mkdir(bundle, { recursive: true })
    await writeFile(join(root, 'package.json'), JSON.stringify({ version: '0.1.0-beta.1' }))
    const file = 'Claude Code Manager_0.1.0-beta.1_x64-setup.exe'
    await writeFile(join(bundle, file), 'installer-fixture')
    await writeFile(join(bundle, `${file}.sig`), signature)
    await assert.rejects(() => prepareRelease({ root, tag: 'v0.2.0', commit: 'a'.repeat(40) }), /Tag/)
    const result = await prepareRelease({ root, tag: 'v0.1.0-beta.1', commit: 'a'.repeat(40) })
    assert.match(await readFile(join(result.output, 'SHA256SUMS'), 'utf8'), new RegExp(sha256('installer-fixture')))
    const metadata = JSON.parse(await readFile(join(result.output, 'latest.json'), 'utf8'))
    assert.equal(metadata.platforms['windows-x86_64'].signature, signature)
    assert.equal(result.files.length, 5)
  } finally { await rm(root, { recursive: true, force: true }) }
})
