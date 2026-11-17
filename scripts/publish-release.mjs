import { readFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { repository, sha256 } from './release.mjs'

export function verifyAsset(asset, { name, digest, tag, existing = false }) {
  if (asset.name !== name) throw new Error(`GitHub renamed release asset: expected ${name}, received ${asset.name}`)
  const expectedUrl = `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`
  if (asset.browser_download_url !== expectedUrl) throw new Error(`Release asset URL mismatch: ${name}`)
  if (asset.digest !== `sha256:${digest}`) {
    throw new Error(existing
      ? `Refusing to replace existing asset with a different or unknown digest: ${name}`
      : `Uploaded asset digest mismatch: ${name}`)
  }
}

export async function publishRelease({ root = process.cwd(), token, tag, fetchImpl = fetch }) {
  if (!token) throw new Error('GH_TOKEN is required')
  const version = JSON.parse(await readFile(join(root, 'package.json'), 'utf8')).version
  if (tag !== `v${version}`) throw new Error('Tag/version mismatch')
  const headers = { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' }
  const base = `https://api.github.com/repos/${repository}`
  async function api(path, options = {}) {
    const response = await fetchImpl(`${base}${path}`, { ...options, headers: { ...headers, ...options.headers } })
    if (!response.ok) throw new Error(`GitHub ${response.status} for ${path}`)
    return response.json()
  }
  const releases = await api('/releases?per_page=100')
  let release = releases.find((item) => item.tag_name === tag)
  if (release && !release.draft) throw new Error('Published releases are immutable; create a new version')
  if (!release) {
    release = await api('/releases', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({
      tag_name: tag, name: `Claude Code Manager ${version}`, draft: true,
      prerelease: version.includes('-'),
      body: await readFile(join(root, '.github/release-notes.md'), 'utf8'),
    }) })
  }
  const checksums = (await readFile(join(root, 'artifacts/release/SHA256SUMS'), 'utf8')).trim().split('\n')
  const files = checksums.map((line) => {
    if (!/^[a-f0-9]{64} {2}[^/\\\r\n]+$/.test(line)) throw new Error('Invalid checksum entry')
    return line.slice(66)
  })
  files.push('SHA256SUMS')
  for (const name of files) {
    const content = await readFile(join(root, 'artifacts/release', name))
    const digest = sha256(content)
    const expected = checksums.find((line) => line.slice(66) === name)?.slice(0, 64)
    if (expected && expected !== digest) throw new Error(`Local artifact changed: ${name}`)
    const existing = release.assets.find((asset) => asset.name === name)
    if (existing) {
      verifyAsset(existing, { name, digest, tag, existing: true })
      console.log(`Verified existing asset: ${name}`)
      continue
    }
    const upload = release.upload_url.replace(/\{.*$/, '') + `?name=${encodeURIComponent(name)}`
    const response = await fetchImpl(upload, { method: 'POST', headers: { ...headers, 'Content-Type': 'application/octet-stream' }, body: content })
    if (!response.ok) throw new Error(`Upload ${name} failed: HTTP ${response.status}`)
    verifyAsset(await response.json(), { name, digest, tag })
    console.log(`Uploaded and verified: ${name}`)
  }
  console.log(`Draft ready for installer validation: ${release.html_url}`)
  return release
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await publishRelease({ token: process.env.GH_TOKEN, tag: process.env.GITHUB_REF_NAME })
}
