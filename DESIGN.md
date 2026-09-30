# Design

How track looks and behaves, and the rules that keep it consistent. The
styles live in `crates/frontend/style/` and the shared components in
`crates/frontend/src/ui/`; this file says how to use them.

## Principles

- **Artwork leads.** Posters, banners and photos carry a page. Chrome around
  them stays quiet: no panels, borders or backgrounds that compete with the
  picture.
- **One primary action.** A card or row has at most one filled accent button
  (usually *Mark watched*). Everything else is a filled secondary button, so
  it is always clear what can be pressed.
- **Soft, not blocky.** Rounded shapes, few borders. Separate things with
  space and surface shades rather than outlines.
- **Say what it is.** Every button has a title, every date says what happened
  ("Aired 3 days ago"), every count has a label. Never show a number that is
  only a placeholder (a count of 0 while loading, an elapsed time of 0s).
- **Phones are first class.** Every page is checked at 400px as well as at
  full width, with touch-sized targets.

## Foundations

### Colour

All colours come from the theme tokens in `_theme.scss` (dark by default, with
a light theme). `_vars.scss` exposes them as Sass variables. Never write a raw
colour literal or use Sass colour functions on a token; mix with
`color-mix(in srgb, <token> N%, transparent)` or add a token.

| Token | Use |
| --- | --- |
| `--surface-0` … `--surface-3` | Page, cards, controls, overlays and hover, stepping up |
| `--surface-glass` | Translucent panels over the page backdrop |
| `--text`, `--text-muted` | Primary text; secondary text, metadata and quiet icons |
| `--accent` (+ `-bg`, `-selected`) | Links, the primary action, selection, the current item |
| `--neutral` (+ `-bg`, `-selected`) | Default buttons and hover fills |
| `--success`, `--danger` (+ `-bg`, `-selected`) | State only (done, failed, destructive), never decoration |
| `--border`, `--border-soft`, `--border-strong` | Dividers, soft outlines, input outlines |
| `--shadow`, `--shadow-strong` | Resting and lifted artwork |

Tinted selections use the accent at low strength: a chip's fill is
`color-mix(… accent 18% …)` with a 45% outline.

### Type

One sans-serif stack; tabular numbers (`font-variant-numeric: tabular-nums`)
wherever digits line up or change (counts, pages, durations, codes).

| Variable | Size | Use |
| --- | --- | --- |
| `$font-title` | 24px | Page headings (`h1`) |
| `$font-xl` / `$font-lg` | 20 / 18px | Section headings |
| `$font-base` | 16px | Body, card titles |
| `$font-sm` | 14px | Controls, lists, secondary text |
| `$font-xs` | 12px | Captions, dates, badges |

Headings and titles use `$bold-weight` (500). Long titles wrap to at most two
lines (`clamp-lines(2)`) rather than being cut to one.

### Space, size and shape

- Spacing: `$padding` (4) / `$padding-md` (6) / `$padding-lg` (8) inside
  controls; `$gap-xs` (8) / `$gap-sm` (12) / `$gap` (16) between them.
- Control height: `$input-height` (32px), `$input-height-touch` (44px) on
  phones. Icon-only buttons are square at that height.
- Radii: `$radius-sm` (6px) for joined groups and badges, `$radius` (8px) for
  controls, `$radius-lg` (12px) for cards, panels and artwork, `999px` for
  pills (chips, page buttons).
- Icons: vendored heroicons, sized with `$icon-size` (18px),
  `$icon-size-touch` (20px) or `$icon-size-sm` (14px), never 100% of their
  container. They take their colour from `--icon-color`.
- Breakpoint: `$mobile-limit` (768px), through the `g.on-mobile` and
  `g.on-desktop` mixins.

### Focus

Everything the keyboard reaches shows one ring on `:focus-visible`: a 2px
`--accent` outline, 2px off the control (the `focus-ring` mixin). Never set
`outline: none` on a focusable element; a wrapper around a bare input shows
the ring itself with `:has(> input:focus-visible)`.

## Components

### Buttons

Every button is `ui::Button`; never write a raw `<button>`. Extend `Button`
when it lacks something.

- `icon` and `title` (the tooltip and accessible name) on every button;
  `label` shows at every width, `text` on phones only, `desktop_text` on wide
  screens only. `children` add content such as a flag.
- Variants: default (neutral), `Primary`, `Success`, `Danger`. Keep `Primary`
  for the one main action and `Danger` for destructive ones.
- `current`: marks the item for the page shown (`aria-current`), for
  navigation and tabs.
- A quick action with a menu is a split button (`MarkTimeMenu quick`): the
  instant action and a `▾` that opens the choices, separated by a divider,
  both full targets.
- The watch-time menu offers chips: *Now*, *Aired* (*Released* for movies)
  and *Custom*, which shows the browser's own date and time fields. Cancel and
  Confirm stay pinned at its foot.

### Tabs

Views of one page (What's Next / Upcoming / Schedule) are an underlined tab bar
(`.page-tabs` > `nav.tabs` > `Button class="tab"`): muted labels, the current one
in text colour with an accent underline. On phones the tabs share the width
equally and drop their icons.

### Chips

Filters are chips (`.chips` > `Button class="chip"`): pills with a thin
outline, tinted accent while on. A count inside a chip is a muted
`.chip-count`. A row of chips stays on one line on phones and scrolls
sideways if it must. `MediaKindToggle` is the shared Shows / Movies pair.

### Controls line and pagination

Above a list, one quiet line (`.page-controls`): what the list holds on the
left ("399 up next"), paging and a *View options* button on the right.
Settings that change rarely (lookahead, page size) go in the View options
popover, not in the line. Pagination is round page buttons with the current
page filled; it hides when there is only one page.

### Popovers, toasts and undo

Popovers are `ui::ContextMenu`: anchored to their trigger and keeping the
side they opened on. Popovers and modals (`ui::Modal`) share one surface:
`--surface-2` (the shade controls are drawn for), a soft border, `$radius-lg` and a `--shadow-strong` shadow.
A modal's header is its title in text colour beside a *Close* button; the page
behind it dims with `--scrim`. On phones a modal is a sheet along the bottom
edge, rounded at the top, and so is every popover: on phones a popover drops
its anchor and opens as a full-width sheet over a dimmed page. A list of actions in a popover is `.menu-list`. Reversible actions happen at once and offer undo
through `Background::offer_undo` and the toast, instead of asking first.
Destructive ones confirm with `ConfirmDanger` in a popover.

### Badges

`.badge` is a short tag before a name: an episode code (`S02E01`) or a task's
kind (`Person`). Use it instead of joining fields with dashes or dots.

### Cards

- On wide screens a card is its artwork: a rounded poster with a soft
  shadow that lifts on hover, then the title (two lines), a badge and name
  line, a labelled date, and the actions, with the primary one first. No panel
  or border around the card.
- On phones a card is a soft panel (`--surface-glass`, `$radius-lg`) with the
  banner across the top, since posters are too tall for a list.
- Cards in a row line up their action rows.

### Status card

A page's current state (the queue's running or next task) is a fixed-height
card: a round badge (spinner while running, clock while waiting, check when
idle, tinted to match), a small caption over the subject, and the page's main
action on the right. It never changes height as the state changes.

### Lists and tables

List-like data is a tight grid with aligned columns that reflows to two lines
on phones, not fields joined inline with separators. Rows that change state
keep their place and their height. Reordering is drag and drop through
`ui::Reorder` and `DragHandle` (pointer and touch, arrow keys on a focused
handle), never up and down buttons.

### Forms

Settings are rows in a `.form-rows` grid, one `ui::FormRow` each: a
sentence-case label beside its control, every control starting on the same
line, with an optional muted hint under it. On phones the label sits above
the control. Content that needs the whole width (a rule editor) goes in a
`.form-wide` block after its row.

Fields (`.input-text`, `.input-number`, `.input-select`) share one look: a
faint `--field-bg` fill and `--field-border` outline mixed from the text
colour, so they read on any surface, stronger on hover and accent while
focused. Selects draw their own chevron; number fields have no spinner and
use tabular digits. A unit or word joined to a field is an `.input-label`
in the same `.input-group`. Captions (`.field label`) are sentence case in
text colour, never uppercase.

### Toggles

On/off settings are `input-checkbox` toggles with a check or cross mark sized
like an icon beside the label.

### Pictures and placeholders

Missing artwork shows a quiet placeholder: a muted person silhouette for
people, the default mark for other images. Photos and posters keep their 2:3
aspect ratio at every width.

## Loading and empty states

- Show `ui::Skeleton` shapes where content will appear, or a spinner for a
  whole list. Don't show counts, page numbers or "nothing here" until the data
  has arrived.
- Empty states are one muted line saying what is missing ("Nothing pending.").

## Words

- Sentence case for labels and buttons ("Sync all", "View options").
- Dates are relative with a verb ("Aired 1 week ago", "Airs in 3 days",
  "Released", "Watched"), with the exact date and time on hover.
- Durations under a second read "Running" or "<0.1s", not "0s".
- No em-dashes between fields; use layout or a badge.

## The app header

The app bar is `#toolbar` and stays at the top of the page. Page headers also
use `.toolbar`, so scope rules for the app bar to `#toolbar`. On phones the
navigation collapses behind a menu button.

## Checking a change

- Look at the page in a browser at full width (about 1250px) and at 400px, and
  wait for its loading indicators to finish before judging it.
- Add a browser test (`crates/e2e`) for the change's key behaviour. Select
  buttons by `title`. When rows are replaced as they update, read their text
  in one snapshot (`rendered_texts`), not element by element.
