# CONTRIBUTING

Thank you for your interest in contributing to Clash Verge Buty! This document covers the local development workflow.

## Development Setup

### Prerequisites

- Windows 10/11 with WebView2 and Visual Studio Build Tools (MSVC).
- A workspace-local toolchain: `.toolchain/` (Node.js, pnpm), `.cargo/` and `.rustup/` (Rust), and `.tmp/` for temporary files. These directories are git-ignored and must not be replaced by system-wide installations. Versions are defined by `package.json` and `AGENTS.md`; see [仓库目录与产物边界](docs/architecture/repository-layout.md).

The repository assumes the workspace-local toolchain is already provisioned. Do not install dependencies into global locations.

### Install Node.js Packages

```shell
node scripts/pnpm.mjs install
```

### Prepare Runtime Assets

Download the Mihomo cores and Geo data, and build the Windows service binaries:

```shell
node scripts/pnpm.mjs run prepare:runtime
```

Existing assets are reused; add `--force` to update them. The step is also run by CI before packaging.

### Run the Development Server

```shell
node scripts/pnpm.mjs run dev
# If an app instance already exists
node scripts/pnpm.mjs run dev:diff
```

### Build and Package

```shell
node scripts/pnpm.mjs run build
```

The full build, verification, and portable packaging workflow is documented in [构建、检查与打包](docs/release/build-and-package.md).

## Contributing Your Changes

1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Commit your changes with clear and concise commit messages.
4. Push your branch to your fork and submit a pull request.

## Documentation

- [Documentation index](docs/README.md)
- [Build, check and package](docs/release/build-and-package.md)
- [Naming](docs/reference/naming.md)
- [Watchdog](docs/architecture/watchdog.md)
