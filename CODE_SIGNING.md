# Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

DiskBreeze has applied to the SignPath Foundation program. Until the application is approved and signing is enabled in the release workflow, Windows builds are unsigned and SmartScreen may warn on first run.

## What gets signed

- The Windows installer `DiskBreeze_x64-setup.exe` (NSIS).
- The portable executable `diskbreeze.exe` inside `DiskBreeze_x64-portable.zip`.

Only binaries built by GitHub Actions from this repository, from the commit tagged for the release, are signed. No third-party binaries are signed with this certificate.

## Team roles

- **Committers and reviewers:** the repository maintainers. Changes from anyone without write access must land through a reviewed pull request.
- **Approvers:** [@SC136](https://github.com/SC136) approves every signing request by hand.
- Everyone with commit, review or approval rights uses multi-factor authentication for GitHub and SignPath.

## Release and signing process

1. A tag `v<version>`, matching the version in `src-tauri/tauri.conf.json`, starts `.github/workflows/release.yml`.
2. CI runs the test suite, then builds the NSIS installer and the portable executable.
3. The unsigned binaries are submitted to SignPath.io as an automated build request and are signed only after an approver approves the request.
4. The signed binaries are published to GitHub Releases, together with `SHA256SUMS.txt`.

## Privacy

DiskBreeze makes one network request: an update check against GitHub when the app starts. It sends no information about the user's files, disk or system, and it can be turned off on the start screen. Other than that, this program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

## Verifying a download

In PowerShell:

```powershell
Get-AuthenticodeSignature .\DiskBreeze_x64-setup.exe | Format-List Status, SignerCertificate
```

`Status` should be `Valid` and the signer should be `SignPath Foundation`. You can also check a download against `SHA256SUMS.txt` from the same release.
