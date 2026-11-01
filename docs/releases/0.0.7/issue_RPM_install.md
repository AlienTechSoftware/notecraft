# Issue 20: RPM installation conflict

## Summary

GitHub issue [#20](https://github.com/AlienTechSoftware/notecraft/issues/20)
reports that the NoteCraft 0.0.7 RPM cannot be installed on a Rocky Linux
system. The RPM transaction fails before the application is installed.

The reported command was:

```text
sudo dnf localinstall ./notecraft-0.0.7-linux-x86_64.rpm
```

The transaction check reports conflicts for shared icon-theme directories:

```text
file /usr/share/icons/hicolor from install of notecraft-0.0.7-1.x86_64
conflicts with file from package hicolor-icon-theme

file /usr/share/icons/hicolor/128x128 from install of notecraft-0.0.7-1.x86_64
conflicts with file from package hicolor-icon-theme

file /usr/share/icons/hicolor/256x256 from install of notecraft-0.0.7-1.x86_64
conflicts with file from package rocky-logos
```

The issue also lists conflicts with directories owned by other installed
packages, including `lightcraft-classic`, `artcraft-launcher` and `gridcraft`.

The failure is a packaging metadata problem. It is not evidence that the
NoteCraft executable failed to start, that a required graphics library is
missing, or that WSLg caused a runtime problem.

## Environment and report timeline

- Product version: NoteCraft 0.0.7.
- Package: `notecraft-0.0.7-linux-x86_64.rpm`.
- Installer: `dnf localinstall`.
- Reported operating system family: Rocky Linux 10.
- Reported installation result: transaction test failure.
- Issue title: `RPM Install incompatibility`.
- Issue: [AlienTechSoftware/notecraft#20](https://github.com/AlienTechSoftware/notecraft/issues/20).
- Follow-up comment: the Linux package had been built and tested under WSLg,
  and a real Linux virtual machine was proposed for additional verification.

The exact package transaction is important. DNF did not report a dependency
resolution failure and did not begin installing files. It stopped during the
RPM transaction test because the package file list claims ownership of paths
that are already owned by installed packages.

## Packaging implementation

The Linux package is assembled by
[`packaging/linux/package.sh`](../../../packaging/linux/package.sh).
The script builds a staged filesystem tree under the package work directory.
It installs:

- The executable at `usr/bin/notecraft`.
- The desktop entry at
  `usr/share/applications/com.alientechsoftware.notecraft.desktop`.
- NoteCraft icons below
  `usr/share/icons/hicolor/<size>x<size>/apps/`.
- License and notice files below `usr/share/doc/notecraft/`.

The icons are created by this loop:

```bash
for size in 32 64 128 256 512; do
    install -Dm644 "$ROOT/assets/brand/notecraft-$size.png" \
        "$STAGE/usr/share/icons/hicolor/${size}x${size}/apps/com.alientechsoftware.notecraft.png"
done
```

The nfpm configuration is in
[`packaging/linux/nfpm.yaml`](../../../packaging/linux/nfpm.yaml).
The executable is mapped as an individual file, but the complete staged
`usr/share` directory is mapped as a tree:

```yaml
contents:
  - src: ${STAGE}/usr/bin/notecraft
    dst: /usr/bin/notecraft
    file_info: { mode: 0755 }
  - src: ${STAGE}/usr/share/
    dst: /usr/share/
    type: tree
```

The second entry is the direct cause of the problem. It asks nfpm to package
the entire `/usr/share` tree instead of listing the files that NoteCraft
actually owns.

## Root cause

### Shared icon directories are system-owned paths

Linux desktop applications conventionally place icons below the shared
freedesktop icon theme hierarchy:

```text
/usr/share/icons/hicolor/
/usr/share/icons/hicolor/32x32/
/usr/share/icons/hicolor/32x32/apps/
/usr/share/icons/hicolor/256x256/
/usr/share/icons/hicolor/256x256/apps/
```

Those directories are not unique to NoteCraft. They are created and owned by
the distribution's icon theme package and are also used by many unrelated
applications.

NoteCraft only owns its icon files, for example:

```text
/usr/share/icons/hicolor/256x256/apps/com.alientechsoftware.notecraft.png
```

It must not claim ownership of the shared parent directories.

### The tree entry expands the package file list too broadly

The `type: tree` entry starts at the staged `usr/share` directory. When nfpm
creates the RPM, the resulting file list can include directory entries for
the complete path hierarchy, including paths such as:

```text
/usr/share
/usr/share/applications
/usr/share/icons
/usr/share/icons/hicolor
/usr/share/icons/hicolor/32x32
/usr/share/icons/hicolor/32x32/apps
/usr/share/icons/hicolor/256x256
/usr/share/icons/hicolor/256x256/apps
```

The NoteCraft icon files have a unique name and are not the conflict. The
conflict is caused by the generated RPM claiming the shared directories
themselves.

On RPM-based distributions, directory ownership is part of package
transaction validation. Existing packages can own those directories, and a
new package that also claims them can be rejected even when the new package
does not replace any existing file.

This explains why the error names `hicolor`, its size directories and its
`apps` directories rather than
`com.alientechsoftware.notecraft.png`.

## Why the issue is distribution-specific

The package was tested in WSLg. That validates a Linux build and can validate
that a window can be shown through the WSLg display integration, but it does
not reproduce a Rocky Linux RPM transaction.

The relevant differences are:

1. WSL development environments are commonly Ubuntu or Debian based.
2. Debian package ownership and RPM package ownership are implemented by
   different package managers.
3. WSLg supplies display sockets and a compositor bridge. It does not provide
   the installed Rocky Linux package database.
4. The reported system already had several packages that owned the shared
   icon hierarchy.
5. A successful Rust build or a successful application launch says nothing
   about whether an RPM can pass a transaction test.

The failure is therefore expected to remain invisible when only WSLg build,
launch and tarball checks are used.

## Why the existing test did not catch it

The Linux packaging smoke test is
[`packaging/linux/test.sh`](../../../packaging/linux/test.sh).
It invokes only the tarball path:

```bash
DIST="$DIST" "$ROOT/packaging/linux/package.sh" --formats tar
```

It then checks that the tar archive contains the executable, desktop entry
and one icon. It does not:

- Build an RPM.
- Inspect the RPM file list.
- Run `rpm --checksig` or another RPM validation command.
- Install the RPM in an RPM-based test environment.
- Run a DNF transaction test with `hicolor-icon-theme` installed.
- Check that shared directories are absent from the RPM file list.

The test passes because tar archives do not perform package ownership
validation. The same staged directory tree can therefore produce a valid
tarball and an invalid RPM.

The packaging implementation was introduced in commit `1d39ee7`, which added
the broad `/usr/share` tree mapping and the tar-only smoke test together.
This left the RPM path without a package-manager-specific regression test.

## Impact

The impact is limited to RPM installation:

- DNF refuses to install the 0.0.7 RPM.
- The application is not installed.
- The desktop entry and icon files are not registered.
- Existing installed applications are not changed because the transaction
  stops before installation.

The following paths are not implicated by the reported failure:

- The NoteCraft Rust executable.
- Notebook folders and page data.
- The desktop entry contents.
- Runtime Wayland or X11 selection.
- WSLg display forwarding.
- The declared shared-library dependencies, which are checked later than the
  file conflict reported in this issue.

The Debian package may not produce the same error, but using the same broad
`/usr/share` tree mapping is still unnecessarily permissive and should be
corrected for both package formats.

## Recommended correction

Replace the broad `/usr/share` tree mapping in
[`packaging/linux/nfpm.yaml`](../../../packaging/linux/nfpm.yaml) with
individual file entries:

1. The desktop entry.
2. Each NoteCraft icon file.
3. The files under `usr/share/doc/notecraft`.

The package should continue to include the shared parent directories needed
for the file paths, but nfpm and the generated package must not list those
directories as package-owned entries when they are already provided by the
system icon theme.

Conceptually, the package contents should be:

```text
/usr/bin/notecraft
/usr/share/applications/com.alientechsoftware.notecraft.desktop
/usr/share/icons/hicolor/32x32/apps/com.alientechsoftware.notecraft.png
/usr/share/icons/hicolor/64x64/apps/com.alientechsoftware.notecraft.png
/usr/share/icons/hicolor/128x128/apps/com.alientechsoftware.notecraft.png
/usr/share/icons/hicolor/256x256/apps/com.alientechsoftware.notecraft.png
/usr/share/icons/hicolor/512x512/apps/com.alientechsoftware.notecraft.png
/usr/share/doc/notecraft/LICENSE-MIT
/usr/share/doc/notecraft/LICENSE-APACHE
/usr/share/doc/notecraft/NOTICE
```

The exact nfpm configuration should be verified against the nfpm version used
by the release process. The acceptance condition is not merely that the
configuration looks narrower. The generated RPM must be inspected to confirm
that it contains NoteCraft's files without claiming the shared `hicolor`
directories.

## Verification plan for the eventual fix

### Static package inspection

After generating the RPM, inspect its contents with the RPM tooling available
in the test environment:

```bash
rpm -qpl ./dist/linux/notecraft-0.0.7-linux-x86_64.rpm
```

The output should contain the NoteCraft desktop entry, icons, executable and
documentation files. It should not contain shared directory entries such as:

```text
/usr/share/icons/hicolor
/usr/share/icons/hicolor/256x256
/usr/share/icons/hicolor/256x256/apps
```

The exact output should be checked because package tools may normalize or
include parent paths differently.

### RPM transaction test

Run the package on a Rocky or Fedora test machine that already has the
standard icon theme installed and at least one other package using the shared
icon hierarchy:

```bash
sudo dnf install ./notecraft-0.0.7-linux-x86_64.rpm
```

The command must complete without a file conflict. A test VM is preferable
to the WSL development environment because it reproduces the target package
database and RPM transaction behavior.

### Regression test coverage

The Linux packaging checks should cover all supported package paths:

- Keep the existing tarball smoke test.
- Build the RPM when `nfpm` is available.
- Inspect the RPM file list.
- Add an RPM transaction test in a Rocky or Fedora container or VM.
- Add the Debian package path to package smoke coverage if it is part of the
  release artifact set.

The test should fail if a future change reintroduces a whole-tree mapping for
`/usr/share`.

## Conclusion

Issue #20 was caused by the RPM package claiming shared icon-theme
directories through the nfpm `type: tree` mapping of all of `/usr/share`.
Rocky Linux correctly rejected the transaction because those directories were
already owned by installed packages.

The narrow fix is to package only NoteCraft-owned files and validate the
generated RPM in an RPM-based environment. No application-code or WSLg
runtime change is required based on the evidence in the issue.

