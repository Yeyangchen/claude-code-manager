# Offline fixtures

The official-manifest.json and official-manifest.json.sig files are unmodified release metadata for Claude Code **2.1.326**, downloaded on 2026-11-16 from:

- https://downloads.claude.ai/claude-code-releases/2.1.326/manifest.json
- https://downloads.claude.ai/claude-code-releases/2.1.326/manifest.json.sig

They contain no binaries, credentials, or user data. Tests verify the genuine detached OpenPGP signature with the pinned Anthropic public key, then reject changed manifest and signature bytes. Preserve the files byte for byte: newline conversion invalidates the signature.
