import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { repository, sha256 } from './release.mjs'

const token = process.env.GH_TOKEN
if (!token) throw new Error('GH_TOKEN is required')
const tag = process.env.GITHUB_REF_NAME
const version = JSON.parse(await readFile('package.json', 'utf8')).version
if (tag !== `v${version}`) throw new Error('Tag/version mismatch')
const headers = { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' }
const base = `https://api.github.com/repos/${repository}`
async function api(path, options = {}) {
  const response = await fetch(`${base}${path}`, { ...options, headers: { ...headers, ...options.headers } })
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
    body: await readFile('.github/release-notes.md', 'utf8'),
  }) })
}
const checksums = (await readFile('artifacts/release/SHA256SUMS', 'utf8')).trim().split('\n')
const files = checksums.map((line) => {
  if (!/^[a-f0-9]{64} {2}[^/\\\r\n]+$/.test(line)) throw new Error('Invalid checksum entry')
  return line.slice(66)
})
files.push('SHA256SUMS')
for (const name of files) {
  const content = await readFile(join('artifacts/release', name))
  const digest = sha256(content)
  const expected = checksums.find((line) => line.slice(66) === name)?.slice(0, 64)
  if (expected && expected !== digest) throw new Error(`Local artifact changed: ${name}`)
  const existing = release.assets.find((asset) => asset.name === name)
  if (existing) {
    if (existing.digest !== `sha256:${digest}`) throw new Error(`Refusing to replace existing asset with a different or unknown digest: ${name}`)
    console.log(`Verified existing asset: ${name}`)
    continue
  }
  const upload = release.upload_url.replace(/\{.*$/, '') + `?name=${encodeURIComponent(name)}`
  const response = await fetch(upload, { method: 'POST', headers: { ...headers, 'Content-Type': 'application/octet-stream' }, body: content })
  if (!response.ok) throw new Error(`Upload ${name} failed: HTTP ${response.status}`)
  const asset = await response.json()
  if (asset.digest !== `sha256:${digest}`) throw new Error(`Uploaded asset digest mismatch: ${name}`)
  console.log(`Uploaded and verified: ${name}`)
}
console.log(`Draft ready for installer validation: ${release.html_url}`)
