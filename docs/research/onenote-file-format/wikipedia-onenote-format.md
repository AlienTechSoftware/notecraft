# Wikipedia: OneNote "File format" section (summary and link trace)

Source: https://en.wikipedia.org/wiki/Microsoft_OneNote (section "File format"
and its references). Read: 2026-10-17. The section text was pasted by the owner;
this file paraphrases it. Wikipedia is a secondary source.

## What the section says

- A notebook is stored as a folder with one data file per section. Files use the
  extension `.one`.
- Wikipedia says a `.one` file "can be a notebook or a section". The Microsoft
  spec contradicts this (see Corrections).
- The file format changed twice after OneNote 2003: in OneNote 2007, then in
  OneNote 2010.
- OneNote 2007 and 2010 open 2003 files read-only, and can upgrade them.
  OneNote 2010 reads and writes the 2007 format, and converts both ways.
- Microsoft documented the format. It is described as an open format that can
  hold varied multimedia in a single `.one` file.
- Several `.one` files can be exported to one `.onepkg` file. A `.onepkg` is a
  cabinet (CAB) archive.

## Corrections from the primary source

- **`.one` vs notebook:** per MS-ONESTORE, `.one` is a section file. A notebook's
  table of contents is a separate `.onetoc2` file. A notebook is the folder plus
  its `.onetoc2`.
- **`.onepkg` contents:** per Wikipedia, `.onepkg` holds the files of one
  notebook in a cabinet archive. Not checked against a Microsoft source.

## Reference trace (Wikipedia refs 21 to 26)

| Ref | Title | Publisher and date given | URL on Wikipedia | Status |
|---|---|---|---|---|
| 21 | [MS-ONE]: OneNote File Format Specification | Microsoft, 2022-12-07 | https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-one/73d22548-a613-4350-8c23-07d15576be50 | Landing page read. Version 3.4 (2022-11-15) is the published one. Body not read. |
| 22 | Opening OneNote 2007 or 2010 files in OneNote 2003 (KB 916888) | Microsoft Support, 2011-09-12 | http://support.microsoft.com/kb/916888 | Not fetched. Old link, may be dead. |
| 23 | Why the OneNote 2007 and 2003 file format are different | David Rasmussen, MSDN Blogs, 2006-10-08 | http://blogs.msdn.com/david_rasmussen/archive/2006/10/08/Why-the-OneNote-2007-and-2003-file-format-are-different.aspx | Not fetched. Old link. Worth a look for design history. |
| 24 | About file format changes in OneNote 2010 | Microsoft Office site, retrieved 2011-09-29 | http://office.microsoft.com/en-us/onenote-help/about-file-format-changes-in-onenote-2010-HA010386956.aspx | Not fetched. Old link, probably dead. |
| 25 | [MS-ONESTORE]: OneNote Revision Store File Format Specification | Microsoft via MSDN, 2011-06-10 | http://msdn.microsoft.com/en-us/library/dd951288%28v=office.12%29.aspx | Old MSDN address. The current edition (v20250520, rev 13.3) was fetched and read from the Microsoft download link. |
| 26 | OneNote 2010: XML Schema Definition (XSD) File | Microsoft Download Center, 2011-09-13 | http://www.microsoft.com/download/en/details.aspx?&id=27396 | Not fetched. A schema for the OneNote 2010 XML (not the binary `.one`); relevant to export or API, not to the file container. |

No other Wikipedia reference in the page concerns the file format.
