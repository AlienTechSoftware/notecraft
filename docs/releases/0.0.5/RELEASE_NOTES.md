# NoteCraft 0.0.5

![NoteCraft 0.0.5 release notes](NC_0.0.5.jfif)

## Highlights

- Corrected Windows installer packaging for the OfficeIMO OneNote import bridge.
- Bridge discovery now supports both installed and portable layouts.
- MSI and portable ZIP builds fail if the bridge is missing.
- Refreshed HTML and CMGFM demo notebooks, including the visible `0.0.5` version.
- Preserved OfficeIMO credit and MIT license links in About.

## Upgrade note

This release replaces `0.0.4`, which is deprecated because its installer could not reliably locate the OfficeIMO bridge. User notebooks are not removed during upgrade.

## Credits

OneNote parsing is provided by [OfficeIMO](https://github.com/EvotecIT/OfficeIMO), distributed under the [MIT license](https://github.com/EvotecIT/OfficeIMO/blob/master/LICENSE). Many thanks to its authors and contributors.
