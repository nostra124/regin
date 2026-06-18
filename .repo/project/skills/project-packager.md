---
name: project-packager
description: |
  Package a project for distribution. Trigger when the
  user wants a deb / rpm / pkgbuild / apk / OCI image.
  Detects host distro, verifies tooling, invokes the
  matching `project package <format>` verb, and
  verifies the resulting artefact.
---

# `project-packager` skill

## 1. Design principles

- **Format follows host.** Default to the format native
  to the running distro. `--format <fmt>` overrides.
- **Verify everything.** Every produced artefact gets
  format-specific introspection (`dpkg --info`, `rpm
  -qip`, `apk info`, etc.) before reporting success.
- **No signing without consent.** Signing keys live in
  `secret`, not `project`. Packager refuses to sign
  unless explicitly authorised.

## 2. The format matrix

| Host distro     | Default | Tool          | Verify with     |
|-----------------|---------|---------------|-----------------|
| debian / ubuntu | deb     | dpkg-deb      | `dpkg --info`   |
| fedora / rhel   | rpm     | rpmbuild      | `rpm -qip`      |
| arch            | pkg     | makepkg       | `pacman -Qip`   |
| alpine          | apk     | abuild        | `apk info`      |
| (any)           | oci     | buildah/docker | image inspect  |

## 3. Workflow recipes

1. **Package for current host.**

       cd ~/src/myapp
       project package                # auto-detect format
       # → myapp_1.0.0_amd64.deb on debian

2. **Package for a specific format.**

       project package rpm
       project package apk

3. **Matrix package (all formats).**

       project package --all

4. **Verify a built artefact.**

       project package --verify myapp_1.0.0_amd64.deb

## 4. Guardrails

1. **Tool availability first.** Verify `dpkg-deb` /
   `rpmbuild` / `makepkg` / `abuild` is on PATH before
   invoking the verb. Don't try `rpmbuild` on a debian
   host without a clear "host mismatch" error.
2. **No signing.** Building unsigned packages is fine;
   signing requires `secret` keys and explicit user
   instruction.
3. **No uploads.** Pushing to apt / dnf / aur / oci
   registries is release tooling, not packaging.
   Decline + redirect.

## 5. Deploying what you built

Packaging without deployment is half a delivery. A project that ships native
packages **must** also ship a `contrib/deploy` script that rolls them out to
remote hosts (probe → build-only-what's-needed → upload → install). See the
`deploy` skill for the contract and `dvalin`'s `contrib/deploy` for the
reference implementation.

## 6. Where to read more

- The `deploy` skill (`operations/deploy.md`) — rolling packages out to hosts
- FEAT-162 (generalised packaging verbs, pending)
- FEAT-166 SIT containers (one per packaging format)
- `man project-package` (pending)
