# clash-astra-service-src (vendored)

This directory vendors the Windows service source implementation used by this repository.

## Attribution

Inspired by upstream projects:

- clash-verge-rev/clash-verge-service
- clash-verge-rev/clash-verge-service-ipc

Only minimal source required by this repository is included here.

## Runtime release policy

- `clash-astra-service.exe`, `clash-astra-service-install.exe`,
  `clash-astra-service-uninstall.exe`, and
  `clash-astra-firewall-helper.exe` are built from this source when creating a
  Clash Astra Runtime Release.
- The generated binaries are packaged into the versioned Runtime Release and
  are not committed to the source repository.
