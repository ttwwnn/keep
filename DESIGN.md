# Keep design

## Theme

Dark, and not by category reflex. The window is a margin around a terminal that
fills a display in a dark room; a light surface beside it would be a lamp. Every
neutral is the terminal's own background colour, read at runtime from the
ghostty config, so the chrome and the content are literally the same colour
rather than a guess at it.

## Colour

Strategy: **restrained**. Tinted neutrals plus one accent, and the accent is
never fixed.

Colours are specified in OKLCH and converted at runtime, so that hues chosen
for different workspaces come out at the same lightness instead of yellow
blazing and blue sinking, which is what an HSL wheel would give.

| Role | Value | Where |
|---|---|---|
| Ground | terminal background (runtime) | sidebar, tab strip, window |
| Ink | white 0.96 | active row, selected tab |
| Ink, resting | white 0.55 | inactive rows |
| Ink, faint | white 0.35 | counts, placeholders |
| Selection | glass in the dark, a wash in the light | the active workspace row |
| Attached | OKLCH(0.76, 0.14, 150) | filled dot |
| Busy | OKLCH(0.82, 0.15, 85) | filled dot |
| Idle | OKLCH(0.70, 0.05, 250) | hollow dot |
| Empty | white 0.30 | hollow dot |

**Selection carries no hue.** It was once a colour per workspace, derived from
the name by a stable hash so that a workspace kept it across sessions. That is
gone: the current row is a pane of the window's own glass in the dark and a
wash in the light, and the hue it used to wear said nothing the name beside it
was not already saying. Colour is left to state alone — the dots — which is
the only thing in the chrome that cannot be read as text.

Where a list has to group rows of one workspace, it groups them by aligning
their names into a column rather than by tinting them. Same information,
and it survives being read by someone who cannot tell the twelve anchors
apart.

## Typography

Two faces, divided by what a string is rather than by where it sits.

- **Identifiers** — workspace names, counts, anything the terminal would also
  print — are set in the terminal's configured face, read from the ghostty
  config at runtime, with a Nerd Font cascade for glyphs an ordinary face has
  no room for.
- **Labels** — placeholders, menu items, anything the app is saying in its own
  voice — are the system face.

Sizes: 12.5 identifiers, 11 counts, 13 labels. Weight carries the active row,
not size.

## Elevation and material

Liquid Glass where the system has it (macOS 26), behind availability checks
with pre-glass fallbacks that are not placeholders but what the app looked like
before. Untinted glass over a dark ground refracts into a well; tinted glass
reads as lit. Those are the two states every control in the chrome has: quiet at
rest, lit under the pointer or when current.

Glass belongs on things that float over content. It does not go on the sidebar
or the strip, which are tinted with the terminal's own colour so that chrome and
content read as one surface.

## Motion

150 to 200 ms, ease out. Motion reports a state change and nothing else: a row
becoming current, a pane going to rest, a size chip arriving and leaving.

## Layout

Dense. Rows are 24 points, gutters are 6, and the list runs to the window's
edges rather than sitting inside a panel. Spacing varies between groups rather
than being uniform: the list breathes at its top, the footer is set apart by a
rule, and a heading in a list carries its gap above the word rather than
around it, so it reads as the start of something.

The picker's rows are 30 rather than 24. They float over content instead of
being furniture, each carries a selection lozenge inset by two, and they are
aimed at with a pointer as often as with the arrows.
