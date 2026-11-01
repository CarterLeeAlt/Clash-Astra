# sysproxy-rs

A library for set/get system proxy. Supports Windows, macOS and linux (via gsettings).

## Tests in this repository

The default suite runs library unit tests and read-only platform checks. It never calls
the live system-proxy setter. Run it with the repository-local toolchain and lockfile:

```powershell
node scripts/pnpm.mjs exec cargo test --manifest-path src-tauri/Cargo.toml -p sysproxy --locked
```

Tests in `src/windows.rs` call the production endpoint parser and restoration
readback logic with in-memory inputs. They cover inactive/stale endpoints,
owned-field mismatches, external-edit protection, finite convergence retries and
persistent readback/query failures without accessing the host registry. These checks
are not proof of live Windows registry restoration.

The Windows disabled-state contract is `enable=false`, empty host and port `0`;
an inactive endpoint must not be compared to its previous enabled endpoint.

This suite contains no live system-proxy write target and needs no additional host
environment. Live Windows network changes require a separately approved acceptance
procedure with the original state captured before any write.
