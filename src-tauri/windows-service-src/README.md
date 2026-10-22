# clash-verge-buty-service-src (vendored)

This directory vendors the Windows service source implementation used by this repository.

## Attribution

Inspired by upstream projects:

- clash-verge-rev/clash-verge-service
- clash-verge-rev/clash-verge-service-ipc

Only minimal source required by this repository is included here.

## Build output policy

- `clash-verge-buty-service.exe`, `clash-verge-buty-service-install.exe`,
  `clash-verge-buty-service-uninstall.exe`, and
  `clash-verge-buty-firewall-helper.exe` are built from this source.
- `src-tauri/local-binaries/windows-service-bin/` is a build output staging directory only.
- Precompiled service `.exe` files must not be committed to Git.
