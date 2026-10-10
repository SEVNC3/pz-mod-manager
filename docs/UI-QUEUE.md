# UI/UX queue (before the next release)

> **Superseded in places.** Later work changed several of these choices:
> - hover and selection now use a white chevron and white text on blood red
> - cards lift by shadow, with no borders
> - the drop line is a feathered glow
> - the game switch and profile select are one menu
>
> The current rules are in the PZMM design system and [RELEASE-1.1-QUEUE.md](RELEASE-1.1-QUEUE.md).

From a contrarian review on 2026-10-10: three critics (typography, visual style,
uniformity) raised 60 findings; a refuter checked each against the code and
screenshots, killed 31 and merged the rest into the 18 items below.

Fixed choices the queue works within: charcoal theme, the blood-red accent
(`#6e080c`, the separator tape's blood), masking-tape separators with the blood
spray, the frameless window, British spelling.

Measured contrast (WCAG):
- `#6e080c` on the backgrounds: 1.52:1 on bg, 1.41:1 on panel, 1.26:1 on raised. Too low for thin indicators, which need 3:1.
- White on `#6e080c`: 12.3:1.
- `--accent-text` at the current 55% mix: 4.45:1 on raised. Moving to a 45% mix gives 5.8:1.

## P1: visible bugs and contrast failures

1. [x] **Lighten `--accent-text` and use it for thin indicators.**
   - Set `--accent-text: color-mix(in oklab, var(--accent) 45%, white)`.
   - Use it for the active tab underline, the drop line (also 3px), the selected-row bar, the busy dot and the best-candidate border in Details.
   - Keep `#6e080c` for primary fills, checkboxes and the progress bar.
2. [x] **Make multi-selected rows visible.** Picked rows use a 22% accent tint plus a 3px `--accent-text` bar. Today's 10% tint is 1.02:1.
3. [x] **Make Workshop star ratings visible.** Lit stars use `--accent-text`. Today lit vs unlit is about 1.1:1.
4. [x] **Stop the blood spray covering the end of tape labels.** Increase `.tape` right padding (about 72px) and/or shrink the spray (`auto 140%`).
5. [x] **Stop long mod names wrapping their id onto a second line.** Make `.name` nowrap with an ellipsis (`max-width: 0` trick) and put the full name in `title`.
6. [x] **Stop long problem text widening the status column and shifting the table.**
   - Truncate `.missing` with an ellipsis.
   - Drop the dead `max-width` and ellipsis rules from `.src`.
   - Replace the `⚠` glyph with `Icon alert`.

## P2: consistency

7. [x] **Hover states.**
   - Button hover border uses `--accent-text`.
   - Primary hover lightens the fill.
   - Inactive segmented-control and tab text is `--muted`, turning `--text` on hover.
8. [x] **Header on one row.** Move Update all, Launch game and Settings from the header into the tab bar, before the links. The list starts about 45px higher.
9. [x] **Keep the tools on the filter row.** The shortcut hint shrinks and truncates, with shorter text. The full list stays in Settings.
10. [x] **Remove the faux bold on the Workshop rank.** Drop `font-weight: 700` from `.rank.top`; Plex Mono is bundled at 400 only.

## P3: polish

11. [x] **One `.link` rule.** It's defined twice in `+page.svelte`. Hints are 12px everywhere (ServersSaves and ZombieBuddy use 13px).
12. [x] **Heading sizes.** ZombieBuddy h2 goes from 24px to 22px to match ServersSaves. LoadOrder h2 gets `line-height: 1.2`.
13. [x] **No width shift when switching tabs.** The active tab drops bold; its colour and underline already mark it.
14. [x] **Copy pass.**
    - Details "Download" becomes "Install".
    - The log says "Installing mod", not "workshop item".
    - Remove confirmations say "Remove", matching the button.
    - "Use this save's mods" everywhere.
    - Workshop "Steam page" buttons become "Workshop page".
15. [x] **Esc closes the import form, the mode plan and Settings.**
16. [x] **Overlap icon.** The "overridden" layer is `--muted`, not `--danger`; being overridden is normal.
17. [x] **Logo.** Recolour the pink cog in `static/brand/pzmm.png` to the blood red. Needs the user's OK: it's a brand asset.
18. [x] **Source badge tooltips.** Subscribed: "Subscribed in Steam; Steam keeps it updated". Workshop: "Downloaded from the Workshop by PZMM".

## Killed (not doing)

These were raised and rejected, with the reasons:
- **Restyling metadata out of mono.** Plex Mono's digits are already tabular, so sizes line up.
- **A six-token type scale and a radius scale.** The 3–8px radius spread isn't visible.
- **One style for all status pills.** Problems and updates should look different from source labels.
- **Renaming the source labels.** Subscribed and Workshop are a real distinction.
- **Hiding the date column.** It changes after updates and is sortable.
- **Custom styling for disabled buttons.** Disabled controls are exempt from contrast rules.
- **Changing the window controls.** They follow the Windows 11 spec.
- **Monochrome brand logos.** Brand logos stay in their own colours.
- **Light-theme contrast tweaks.** The light theme is secondary.
- **Icon size unification.** 14px and 15px side by side look the same.
- **A plural helper.** Not needed yet.
