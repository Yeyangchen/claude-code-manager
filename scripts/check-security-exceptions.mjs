import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const lock = readFileSync('src-tauri/Cargo.lock', 'utf8')
for (const [name, reviewedVersion] of [['pgp', '0.21.1'], ['rsa', '0.9.10'], ['crypto-bigint', '0.5.5']]) {
  const packages = lock.split('[[package]]').filter((block) => block.includes(`\nname = "${name}"\n`))
  assert.equal(packages.length, 1, `${name} graph changed; reassess security exceptions`)
  assert.match(packages[0], new RegExp(`\nversion = "${reviewedVersion.replaceAll('.', '\\.')}"\n`), `${name} version changed; reassess security exceptions`)
}
console.log('Verify-only security exceptions match reviewed versions. See docs/security-audit.md.')
