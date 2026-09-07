# Super Platinum design system

Super Platinum uses **Ink & Signal**: quiet near-black planes, cool-gray structure,
and one electric-blue interaction language. It should remain recognizably Slack
in information architecture while feeling as immediate as Telegram and as
disciplined as Vercel's product UI.

The implementation source of truth is:

- `src/desktop/styles/base.css` — primitive tokens
- `src/desktop/styles/design-system.css` — shared component and motion standards
- `src/desktop/view/theme.rs` — user-selectable palette values
- feature stylesheets — layout and behavior unique to that feature

## Principles

1. **Conversation first.** Chrome recedes; messages, people, and unread state lead.
2. **One signal color.** Blue means selected, focused, linked, or actionable. Red,
   amber, and green are reserved for destructive, warning, and success/status states.
3. **Dense, not cramped.** Default body text is 14px. Metadata is 11px. Controls
   are 32px high. Use the 4/8/12/16px spacing scale before inventing a value.
4. **Shape communicates layer.** 5px for rows, 8px for controls, 12px for floating
   surfaces. Pills are only for counts, reactions, and status.
5. **Motion explains state.** Animate occasional overlays and direct press feedback,
   not scrolling, keyboard navigation, or frequently visited rows.

## Foundations

### Typography

Use `--font-sans` everywhere except code, which uses `--font-mono`.

| Role | Token | Use |
| --- | --- | --- |
| Metadata | `--text-xs` / 11px | timestamps, shortcuts, counts |
| Utility | `--text-sm` / 12px | tabs, labels, secondary controls |
| Body | `--text-md` / 14px | messages, navigation, form values |
| Title | `--text-lg` / 16px | modal and major surface titles only |

Use 560–680 weights for hierarchy. Avoid bold body copy and arbitrary 13px/15px
sizes. Use tabular numerals for time, storage, and changing counts.

### Color and elevation

`--bg` is the canvas; `--surface` is an embedded work area;
`--surface-raised` is interactive elevation. Use spacing and surface contrast to
separate content before reaching for a border. `--shadow-popover` is only for
content floating above the shell. Gradients are not decoration and selection is
a quiet, uniform fill.

### Component model

Treat these as the stable UI vocabulary, even where the current renderer still
uses feature-specific class names:

| Component | Existing implementations | Contract |
| --- | --- | --- |
| Selection row | `.channel`, `.nav-row`, `.palette-result`, `.dm-row` | One uniform fill; never an edge stripe or outline |
| Icon button | `.rail-btn`, `.icon-btn`, `.composer-icon-btn` | Fixed square hit area; tint on hover; subtle press scale |
| Text field | palette/search inputs, `.list-find` | Filled surface at rest; focus ring only while focused |
| Segmented control | `.settings-tabs` / `.settings-tab` | Shared track fill and active fill; no nested outlines |
| Floating surface | `.modal`, `.self-menu`, `.message-tools` | Opaque raised fill and shadow; no decorative frame |
| Composer | `.composer-wrap`, `.thread-composer` | Raised work surface with a focus ring, stable dimensions |
| Status pill | badges and `.reaction` | Reserved for compact state/counts, never general containers |
| Attachment | `.attachment-body`, `.attachment-chip`, `.pending-file` | One filled surface; progress changes fill, not frame weight |
| Notice | `.toast` | Raised, bounded copy with an explicit dismiss control; never cover the composer |
| Empty state | `.main-empty`, `.list-empty`, `.secondary-empty` | Direct guidance without decorative placeholder tiles |
| Profile surface | `.profile-hover`, `.profile-pane` | Shared identity hierarchy and actions; details remain scrollable |

When a pattern gains a second behavioral consumer, extract a Dioxus component in
`src/desktop/view/` instead of copying its RSX. A component owns semantics,
keyboard behavior, states, and stable class hooks; feature styles own placement.
Do not create presentation-only wrappers that merely rename one `div`.

### Borders and selection

Borders are reserved for structural splits that help navigation, explicit focus,
and content whose bounds would otherwise be ambiguous. Do not frame every nested
surface, form row, tab, or swatch.

**Never use a left-edge stripe, inset edge, gradient edge, or similar flourish for
selected or unread state.** Selection uses a uniform background fill and text
contrast. Unread state uses weight, a count/dot, or a uniform low-contrast fill.

Custom backgrounds always paint beneath a translucent timeline surface and never
repeat. User-selected dimming can change the wallpaper, but it cannot remove the
minimum text-legibility layer.

Every text/background pairing should meet WCAG AA. Never put `--muted-2` on an
accent-colored surface or use color alone for status.

### Motion

| Interaction | Duration | Easing | Properties |
| --- | --- | --- | --- |
| Hover/color feedback | `--duration-fast` / 120ms | `ease` | color, background, border |
| Button press | 120ms | `--ease-ui` | transform only |

Do not animate width, height, padding, margin, blur, list selection, or timeline
scrolling. Dialog entrance motion is intentionally omitted: the pinned native
WebView does not reliably composite newly inserted animated overlay layers. Every
new animation needs an explicit `prefers-reduced-motion` rule and a native capture.

## Canonical mocks

These are structural contracts, not alternate implementations. The real Dioxus
fixtures are the reviewable mocks and must be used for visual changes.

### Conversation shell — `main-general-new-message-motion`

```text
┌────┬───────────────┬──────────────────────────────────────┐
│rail│ workspace     │ # channel                    actions │
│    ├───────────────┼──────────────────────────────────────┤
│    │ Jump to…  ⌘K │                                      │
│    │ Unreads       │ avatar  Name · time                  │
│    │ Threads       │         Message body leads.          │
│    │               │         reactions                    │
│    │ CHANNELS      │                                      │
│    │   # selected  │                                      │
│    │   # channel   ├──────────────────────────────────────┤
│    │               │ ＋  Message #channel             send │
└────┴───────────────┴──────────────────────────────────────┘
```

### Jump palette — `palette`

```text
                 ┌──────────────────────────────┐
                 │ Channel or person            │
                 ├──────────────────────────────┤
                 │ RECENT                       │
                 │   avatar  Selected result  1 │
                 │   #       Channel             │
                 └──────────────────────────────┘
```

### Settings — `settings` and `settings-storage`

```text
                 ┌──────────────────────────────┐
                 │ Settings                  ×  │
                 │ ┌ Appearance ┬ Storage ┐     │
                 │ Theme            [ value ]   │
                 │ Density          [ value ]   │
                 │ ─────────────────────────    │
                 │ Sidebar width    ━━━━━●━━    │
                 └──────────────────────────────┘
```

Run `scripts/agent-ui-check.sh`, then inspect the corresponding PNGs under
`tmp/agent-ui/`. A screenshot is required for visual review, but behavior still
needs the relevant Rust tests.

## Agent checklist

- Reuse tokens before adding values. Add a token only when at least two components
  share the concept.
- Keep feature layout in its existing stylesheet; put truly shared interaction
  language in `design-system.css`.
- Selected state uses a uniform tint; hover uses a weaker tint; focus uses a
  focus ring. Never add a decorative edge or stripe to selection or unread state.
- Controls keep their dimensions across hover/loading states.
- Validate keyboard focus, narrow panes, long labels, reduced motion, and both
  default and non-default fixture states.
