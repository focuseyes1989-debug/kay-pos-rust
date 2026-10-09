# Receipt Editor

## Phase 1

Settings → Receipt supports 58, 80, 112 mm presets and custom widths from 40–120 mm. Printable width is separate from roll width; the Windows driver must support the requested media. Left/right margins reduce printable content width; at least 10 mm must remain. Top/bottom margins and section gaps use mm.

Logo width/height are maximum bounds, preserving aspect ratio. Alignment and gap after the logo are configurable. Eight text blocks expose font size (6–24 pt), bold, alignment, line spacing (1–3), and gap after (0–20 mm). Address/header/footer fields support multiple lines.

The live sample and sale-saved preview share receipt rows with actual printing and historic reprints. Save persists draft settings; Reset Changes discards edits. Reset layout restores layout defaults while retaining shop text and image. Test Print prints the unsaved draft using the configured receipt printer; it does not record a sale. Long receipts paginate within the driver length limit.

Validation: `cargo test -p pos_desktop --offline --bin pos_desktop receipt_` and `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-receipt-editor.ps1`. The script tests real GDI wrapping, Burmese text, line spacing, pagination and paper bounds without spooling. Physical printer output still requires a Test Print on the target printer.

## Phase 2

- Fields & order: show/hide the 17 available receipt fields, and move them with ↑/↓. Items stay grouped. Empty or inapplicable values (e.g. zero tax or balance due) remain omitted. Reordering rebuilds separators between sections.
- Item columns: choose the existing two-line layout or a table. Item name, quantity, unit price and amount can be shown/hidden. In table mode, change their order and relative width weights; visible columns automatically share the printable content width. Each column must have at least 3 mm. Totals label/value split can be set from 20–80%.
- Templates: Standard, Compact and Item columns presets retain the current paper/margin settings. Enter a name and Store template to retain a custom layout in the draft (up to 20). Matching names replace existing templates. Apply loads a saved layout; delete removes it from the draft. Templates contain layout choices, not shop text, images or printer selection. Reset layout retains the template library.
- Click Save settings to persist any changes, including the template library. Preview and Test Print use the current draft before saving. Shared rows carry visibility, order, widths and content for new receipts and historic reprints. Display choices never change recorded sale amounts.

Validation also exercises production multi-column PrintPage pagination without spooling, field visibility/order, currency totals, template round-trip and invalid layouts. Physical printer output requires Test Print on the target printer.

## Phase 3

Drag the six-dot grip on Fields & order, Item columns, or a block in the live sample preview. The blue line marks insertion before/after the target. All rows belonging to Items move as one block. Field ordering and column ordering remain separate; dropping outside the relevant targets cancels. Hidden/empty fields can still be moved in the field list. Historic/sale-saved preview does not expose editing controls.

Mouse, pen and touch use pointer events; only the grip disables native scrolling, so the rest of the page remains scrollable. Near a scrollable pane edge, dragging auto-scrolls. No order is committed until release. Escape, pointer cancellation, losing capture/window focus, or starting a save cancels the move. A small movement threshold avoids accidental moves on a tap.

Keyboard: focus a grip, press Space/Enter, choose the position using arrow keys, then Enter/Space to drop. Escape/Tab cancels. Existing ↑/↓ buttons remain available. Status announcements and focus restoration support keyboard navigation.

Reordered layout uses the existing draft, Save settings, Reset Changes and template mechanisms. Preview and the printer share the persisted order; print dimensions and sale data are not altered by dragging.

Validation: receipt Rust tests cover all before/after permutations, invalid targets, template persistence and receipt/reprint equivalence. `scripts/check-receipt-drag.cjs` runs the production browser handler against a matching DOM fixture in headless Edge using Playwright (mouse, keyboard/focus, simulated touch, preview item grouping, disabled/cancel, domain isolation and auto-scroll). `scripts/check-receipt-editor.ps1` verifies GDI output geometry and pagination without spooling.

No release or update is included in Phases 1–3. Physical printer output requires Test Print on the target printer; simulated touch is not a physical touchscreen check.
