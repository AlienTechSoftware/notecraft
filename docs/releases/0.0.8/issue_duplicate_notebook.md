# Duplicate Notebook in Sidebar Bug

- Date: 2026-10-31
- Status: Bug report - found in testing
- Branch: `feat/v0.0.8`
- Severity: High (breaks UI, confuses users)

---

## Problem

The "Welcome to NoteCraft" notebook appears **twice** in the sidebar notebook list.

**Screenshot evidence:**
- Shows two "Welcome to NoteCraft" entries in NOTEBOOKS panel
- User sees duplicate, has to figure out which one to click
- Breaks the assumption that each notebook appears once

---

## Questions to Investigate

1. Is it in the file system twice? (duplicate folder)
2. Is it being added twice in code? (session.refresh() issue?)
3. Is it a rendering bug? (tree not deduplicating)
4. Is it related to the Welcome notebook auto-deployment we removed?

---

## Next Steps

1. Check the notebooks folder structure on disk
2. Search for where notebooks are loaded/added to the tree
3. Check if `session.refresh()` or similar is adding duplicates
4. Look at the notebook list rendering code

---

## Related

- Issue #18 removed Welcome notebook auto-deployment
- May be fallout from that change, or pre-existing bug
