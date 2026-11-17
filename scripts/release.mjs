import { createHash } from 'node:crypto'
import { copyFile, mkdir, readdir, readFile, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

export const repository = 'Yeyangchen/claude-code-manager'
export const sha256 = (content) => createHash('sha256').update(content).digest('hex')

export function installerFileName(version) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(version)) throw new Error('Invalid release version')
  return `claude-code-manager_${version}_x64-setup.exe`
}

export function updaterManifest(version, fileName, signature, date) {
  if (fileName !== installerFileName(version)) throw new Error('Invalid x64 installer filename')
  const decoded = Buffer.from(signature.trim(), 'base64').toString('utf8')
  if (!decoded.startsWith('untrusted comment:') || !decoded.includes('trusted comment:')) throw new Error('Missing Tauri installer signature')
  return {
    version,
    notes: `Claude Code Manager ${version}. 非官方社区工具。发布说明见 GitHub Release。`,
    pub_date: date,
    platforms: {
      'windows-x86_64': {
        signature: signature.trim(),
        url: `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(fileName)}`,
      },
    },
  }
}

export async function prepareRelease({ root = process.cwd(), tag, commit, date = new Date().toISOString() }) {
  const pkg = JSON.parse(await readFile(join(root, 'package.json'), 'utf8'))
  if (tag !== `v${pkg.version}`) throw new Error('Tag does not match package version')
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error('A complete source commit SHA is required')
  const bundle = join(root, 'src-tauri/target/release/bundle/nsis')
  const candidates = (await readdir(bundle)).filter((name) => name.endsWith('_x64-setup.exe'))
  if (candidates.length !== 1) throw new Error('Expected exactly one x64 NSIS installer')
  const sourceName = candidates[0]
  if (!sourceName.includes(`_${pkg.version}_`)) throw new Error('Installer version mismatch')
  const name = installerFileName(pkg.version)
  const signature = await readFile(join(bundle, `${sourceName}.sig`), 'utf8')
  const manifest = updaterManifest(pkg.version, name, signature, date)
  const output = join(root, 'artifacts/release')
  await mkdir(output, { recursive: true })
  await copyFile(join(bundle, sourceName), join(output, name))
  // The signature authenticates the installer bytes. Retain its original trusted comment.
  await copyFile(join(bundle, `${sourceName}.sig`), join(output, `${name}.sig`))
  await writeFile(join(output, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
  await writeFile(join(output, 'provenance.json'), `${JSON.stringify({ repository, tag, commit, windowsAuthenticode: 'unsigned', updaterSignature: 'Tauri minisign', date }, null, 2)}\n`)
  const names = [name, `${name}.sig`, 'latest.json', 'provenance.json'].sort()
  const checksums = await Promise.all(names.map(async (file) => `${sha256(await readFile(join(output, file)))}  ${file}`))
  await writeFile(join(output, 'SHA256SUMS'), `${checksums.join('\n')}\n`)
  return { output, files: [...names, 'SHA256SUMS'], version: pkg.version }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const result = await prepareRelease({ tag: process.env.GITHUB_REF_NAME, commit: process.env.GITHUB_SHA })
  console.log(`Prepared ${result.files.length} assets for ${result.version}`)
}
