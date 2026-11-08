# NoteCraft v0.0.9 Release Notes

## Highlights

- **Android:** First Android release, with thanks to [MostafaXS1](https://github.com/MostafaXS1).
- **Infinite canvas:** Draw in open page space beyond text blocks.
- **Languages:** Interface localization across 13 locales, with French and Spanish User Assistance.
- **Page organization:** UI improvements for creating pages and promoting them to sub-pages.
- **First-use setup (Windows):** After installation, or after a preferences reset, NoteCraft opens one setup dialog: a welcome page with the banner, the local logging level, your page format (HTML or CommonMark + GFM), whether to add the Welcome notebook in that format (13 languages), and a thank-you page. It does not appear again for later app versions. Other notebooks are never replaced, and installation copies no notebook content. macOS, Linux, the website and Android are deferred for this flow in 0.0.9.
- **Importing:** Import test dataset and preserve section colours.

## About NoteCraft

NoteCraft stores notebooks in plain folders and pages in ordinary files. Choose HTML for rich
layout or CommonMark + GFM for readable diffs and merges. Pages and their media stay together,
so you can use the sync and backup tools you already trust.

## Release preview

The animated preview is generated from the supplied background by the Pillow script:
[view the release GIF](RELEASE_NOTES.gif). To regenerate it, run:

```sh
python docs/releases/0.0.9/0.0.9.py
```

The [individual slide images](frames/01-android-release.png) are available for inspection.

## Contributors

Code and packaging contributions: [tanmaysinghx](https://github.com/tanmaysinghx),
[MPro87](https://github.com/MPro87), and [rastacalavera](https://github.com/rastacalavera).
Android application collaboration: [MostafaXS1](https://github.com/MostafaXS1).

Thank you to issue reporters [AKRking](https://github.com/AKRking),
[Christian-Espinosa](https://github.com/Christian-Espinosa), [AshOtis](https://github.com/AshOtis),
[mgrenonville](https://github.com/mgrenonville), [Virility](https://github.com/Virility),
[Mathew-D](https://github.com/Mathew-D), [sl-books](https://github.com/sl-books),
[BYIIHub](https://github.com/BYIIHub), [MSTSmusic](https://github.com/MSTSmusic), and
[Devilmayycryyy](https://github.com/Devilmayycryyy). MPro87's issue reports are included with
their contributor credit in [the detailed credits](CONTRIBUTORS.md).

See the [detailed v0.0.9 contributor credits](CONTRIBUTORS.md) for contributions and issue links,
and the [contributor guide](https://github.com/AlienTechSoftware/notecraft/blob/main/CONTRIBUTING.md)
to learn how to contribute.

## Online

- [Project profile](https://notecraft.cloud/)
- [Reddit community](https://www.reddit.com/r/notecraftai/)
- [Playground](https://playground.daijro.dev/head/notecraft/)
- [GitHub repository](https://github.com/AlienTechSoftware/notecraft)
- [Discord](https://discord.com/channels/1359579021108842617/1559147620084088982)

