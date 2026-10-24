# clash-astra-service-src (vendored)

This directory vendors the Windows service source implementation used by this repository.

## Attribution

Inspired by upstream projects:

- clash-verge-rev/clash-verge-service
- clash-verge-rev/clash-verge-service-ipc

Only minimal source required by this repository is included here.

## Build output policy

- `clash-astra-service.exe`, `clash-astra-service-install.exe`,
  `clash-astra-service-uninstall.exe`, and
  `clash-astra-firewall-helper.exe` are built from this source.
- `src-tauri/local-binaries/windows-service-bin/` is a build output staging directory only.
- Precompiled service `.exe` files must not be committed to Git.
