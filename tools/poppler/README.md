# Bundled Poppler runtime

Windows release packaging stages Poppler's `pdftoppm` under
`renderers/poppler/Library/bin`. The archive is downloaded from the Poppler Windows
release project with a pinned SHA-256 checksum by `package.ps1`.

Poppler and its dependent libraries retain their upstream licence files. The release
bundle must ship those files beside the staged runtime.
