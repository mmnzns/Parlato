# Parlato — handoff notes

Parlato is a fork of **Parla** by Florian (littlerabbit), used and customized with permission. The design source is `Engine-1a-Workbench.dc.html` (Workbench direction). Tokens: `handoff/index.css`.

## 1. Rename Parla → Parlato (code, not just copy)
- [ ] `package.json` name/productName
- [ ] Tauri/Electron config: `productName`, window title, app identifier (e.g. `com.<you>.parlato`)
- [ ] Tray tooltip + tray menu labels
- [ ] Installer name, Start-menu shortcut, uninstall entry, "Start with Windows" registry/autostart key
- [ ] All three locale files — every "Parla" string, not just English
- [ ] Assistant wake phrase: "hey parla" → "hey parlato" in the **matching logic**, not only the label. Accept both for one release.
- [ ] Keep internal names that users never see (e.g. CSS keyframes `parlaBar`, `parlaDot`) — optional to rename.

## 2. Existing users' data
If the app identifier or app-data folder changes, settings, history, dictionary and downloaded models won't be found. On first launch:
- [ ] Look for the old Parla data folder; if present and no Parlato folder exists, copy (don't move) it over.
- [ ] Don't re-download speech models that already exist in the old folder.

## 3. Icons
- Source: `public/parlato-icon.png` (256px, cropped). Pre-sized PNGs: `handoff/icons/parlato-{16,24,32,48,64,256}.png`.
- [ ] Build `icon.ico` containing 16/24/32/48/64/256.
- [ ] Tray: the 16px icon reads as a solid orange "P" — the sound bars and green dot disappear. Acceptable, but test on both light and dark taskbars. If it looks muddy, make a single-color tray glyph (just the P).
- Full-resolution originals (icon + wordmark, dark/light): `uploads/Parlato *.png`.

## 4. Fonts
Bundle offline — don't load from Google Fonts. Steps at the top of `handoff/index.css` (`@fontsource-variable/bricolage-grotesque`, `@fontsource/ibm-plex-mono`).

## 5. Credit & licence
- [ ] Keep the original LICENSE file (and add your own copyright line beneath it, if the licence allows).
- [ ] Settings › About includes "Based on Parla by Florian (littlerabbit)" with a link to the original repo (already in the design).

## 6. Accessibility (already in the design — keep it when porting)
- Focus ring on every button/input/textarea/label: `outline: 2px solid var(--ring); outline-offset: 2px` on `:focus-visible`.
- Muted text contrast checked: light `#5C5B55` and dark `#A5A49D` both pass 4.5:1 on their sidebar/background.
- Radios/checkboxes/toggles use `role` + `aria-checked`; keep those.

## 7. Navigation
Sidebar groups: (unlabeled) Home, History · **set up** Microphone & shortcut, Speech model, AI cleanup, Settings · **more** Power modes, Dictionary, Transcribe a file. The "more" group is always visible — not collapsible.
