# CONTRIBUTING

Thank you for your interest in contributing to Clash Verge Buty! This document provides guidelines and instructions to help you set up your development environment and start contributing.

## Development Setup

Before you start contributing to the project, you need to set up your development environment. Here are the steps you need to follow:

### Prerequisites

1. **Install Rust and Node.js**: Our project requires both Rust and Node.js. Please follow the instructions provided [here](https://tauri.app/v1/guides/getting-started/prerequisites) to install them on your system.

### Setup for Windows Users

If you're a Windows user, you may need to perform some additional steps:

- Make sure to add Rust and Node.js to your system's PATH. This is usually done during the installation process, but you can verify and manually add them if necessary.

### Workspace-local toolchain

This repository uses a toolchain that lives inside the working copy: `.toolchain\` (Node.js 24.18.0 and pnpm 11.15.1), `.cargo\` and `.rustup\` (Rust), and `.tmp\` (temporary files). They are git-ignored and must not be replaced by system-wide installations. Run every pnpm command through `node scripts/pnpm.mjs`, never through a system-wide `pnpm`. pnpm 11 reads install locations from `pnpm-workspace.yaml` (`storeDir`, `cacheDir`, `stateDir`), not from `.npmrc`; older pnpm versions ignore that file and put the store at the drive root (for example `D:\.pnpm-store`). `node scripts/check-workspace-hygiene.mjs` verifies the layout.

### Install Node.js Packages

After setting up the workspace-local toolchain, install the necessary Node.js packages:

```shell
node scripts/pnpm.mjs install
```

### Download the Mihomo Binary

You have two options for downloading the Mihomo binary:

- Automatically download it via the provided script:
  ```shell
  node scripts/pnpm.mjs run check
  # Use '--force' to force update to the latest version
  # node scripts/pnpm.mjs run check --force
  ```
- Manually download it from the [Mihomo releases](https://github.com/MetaCubeX/mihomo/releases). After downloading, rename the binary according to the [Tauri configuration](https://tauri.app/v1/api/config#bundleconfig.externalbin).

### Run the Development Server

To run the development server, use the following command:

```shell
node scripts/pnpm.mjs run dev
# If an app instance already exists, use a different command
node scripts/pnpm.mjs run dev:diff
```

### Build the Project

If you want to build the project, use:

```shell
node scripts/pnpm.mjs run build
```

## Contributing Your Changes

Once you have made your changes:

1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Commit your changes with clear and concise commit messages.
4. Push your branch to your fork and submit a pull request to our repository.

We appreciate your contributions and look forward to your active participation in our project!
