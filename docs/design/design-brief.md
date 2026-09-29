# Parla (MNM) - UI Design Brief

Brief for exploring a new visual direction in Claude Design. The goal is a
design that can be implemented once, cleanly, instead of being tuned through
many small edits afterwards.

## What Parla is

A Windows desktop app that turns speech into text and pastes it where the cursor
is. It lives in the system tray. Users open the main window to configure it, and
see a small floating "pill" while they are recording.

Audience: people who dictate a lot (writers, professionals, developers). It
should feel like a focused, trustworthy productivity tool.

## Direction (to be decided in Claude Design)

- Feel / personality: _(fill in: e.g. calm and premium, warm and friendly, sharp pro tool)_
- Color: _(fill in: brand colors, references, anything to avoid)_
- Must look clearly different from the upstream Parla, which uses the stock
  shadcn/ui look: pure greys, black primary buttons, no accent color.

## Deliverables that make implementation straightforward

The app is styled through a small set of design tokens. A design that specifies
these exactly can be applied in one pass. Please produce:

1. **Color tokens, light AND dark**, a value for each:
   background, foreground, card, card-foreground, popover, popover-foreground,
   primary, primary-foreground, secondary, secondary-foreground, muted,
   muted-foreground, accent, accent-foreground, destructive,
   destructive-foreground, border, input, ring.
   Hex or OKLCH both fine.
2. **Corner radius**: one base value (currently 10px; small/medium/large/xl derive from it).
3. **Typography**: font family, plus sizes used for page titles, section
   headings, body, and small helper text. The app mostly uses small text
   (14px body, 12px helper text) because the settings screens are dense.
4. **Component styles**: buttons (primary, secondary, outline, ghost,
   destructive), cards/sections, text inputs and dropdowns, toggles, and the
   sidebar navigation item in its normal, hover and selected states.
5. **Three key screens mocked** with the new style (these cover the patterns
   used everywhere else):
   - **Dashboard**: overview / stats
   - **Enhancement**: a dense settings form (provider dropdown, model dropdown,
     API key field, text inputs, help text)
   - **History**: a list of past transcriptions
6. **The recorder pill**: a small floating bar (about 184px wide, 300px when
   showing live text), always dark today, with a record/stop button, an audio
   level meter, and a mode button.

## Constraints (keep the design implementable)

- **Restyle, don't redesign features.** Same screens, same controls, same flows.
  Layout and spacing can change; adding new features or moving functionality
  between screens is out of scope for this pass.
- **Built on shadcn/ui + Tailwind CSS.** Anything expressible as colors, radius,
  spacing, typography, borders and shadows is cheap. Heavy custom illustration,
  complex animation or unusual components are expensive.
- **Icons are Lucide** (lucide-react). Keep Lucide, or name a comparable set.
- **Fonts must ship with the app.** Parla works fully offline, so a custom font
  needs to be a bundleable file (e.g. from Google Fonts, open license), not a
  web link. The Windows system font (Segoe UI) is also a valid choice.
- **Text is translated** into English, French and Spanish. French and Spanish
  labels run 20-30% longer, so buttons and navigation need room.
- **Window size**: opens at 1100 x 720, minimum 900 x 600. Native Windows 11
  title bar stays.
- **Layout today**: a left sidebar with 10 items (Dashboard, Transcribe,
  History, AI Models, Enhancement, Power Mode, Permissions, Recorder,
  Dictionary, Settings) and a content area.

## Open decision

**Dark mode.** The main window is currently light-only (dark tokens exist in the
code but are never switched on). Options:
- Light only (no extra work)
- Dark only
- Follow the Windows system theme (a small extra feature: detect the theme and
  switch)

Decide this in design, since it determines whether both token sets need to be
polished or just one.

## Handoff back to Claude Code

When the direction is settled, bring back: the token values (item 1-3), the
mocked screens (screenshots or Claude Design's handoff export), and the
dark-mode decision. Implementation order: tokens first (one file, changes the
whole app at once), then component styling, then any per-screen layout changes.
