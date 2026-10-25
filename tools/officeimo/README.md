# OfficeIMO release bundle

NoteCraft loads the optional OfficeIMO bridge from:

```text
<NoteCraft data directory>/importers/officeimo/<rid>/officeimo-notecraft[.exe]
```

Build a self-contained release bundle with:

```powershell
pwsh tools/officeimo/package.ps1 `
  -BridgeProject path/to/OfficeIMO.NoteCraft.Poc.csproj `
  -Rid win-x64 `
  -Version 0.0.6 `
  -OutputRoot path/to/release
```

Run it once for each supported RID: `win-x64`, `linux-x64`, `osx-x64`, and `osx-arm64`.
The script records the SHA-256 of each executable in `manifest.json`. Release packaging must
also copy the OfficeIMO and transitive dependency license files beside that manifest. Do not
commit generated binaries to the source repository.
