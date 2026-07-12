# PageForm Sticky Footer Design

## Problem

The shared `PageForm` footer uses a negative sticky bottom offset equal to the shell page gap. After page scrolling moved into an independently clipped transition layer, that offset places part of the footer below the visible scrollport whenever the form is not at its natural bottom position. The footer becomes fully visible only after scrolling to the end.

## Scope

Fix the shared `PageForm` component so every current consumer receives the same behavior:

- provider create and edit forms;
- key create and edit forms;
- channel monitor create and edit forms.

Business form state, submission behavior, footer content, horizontal full-bleed styling, and page header behavior remain unchanged.

## Design

Keep the existing full-width footer presentation and bottom margin compensation. Change only the sticky constraint from the negative shell-gap offset to `bottom-0`, anchoring the complete footer inside its scrollport at every scroll position.

This is preferred over restoring the entire older footer style because it preserves the current horizontal alignment. A wrapper-and-spacer redesign is unnecessary for a positioning regression with a single confirmed cause.

## Regression Coverage

Add a focused source-contract test before changing production code. The test will require the shared footer to use `sticky bottom-0` and reject the negative bottom-offset class that caused clipping. It will also confirm that the affected form pages continue to consume the shared `PageForm` rather than carrying page-specific footer positioning.

Verification will include:

- the focused regression test, observed failing before the fix and passing after it;
- the relevant script test suite;
- TypeScript and Vite build checks;
- browser inspection at the top, middle, and bottom of a scrollable form, confirming that the footer stays fully visible and stable.

## Success Criteria

- The complete footer border, controls, and vertical padding remain visible before reaching the bottom of a form.
- Scrolling to the bottom does not change the footer's vertical position or height.
- All `PageForm` consumers inherit the correction without page-specific overrides.
- No unrelated working-tree changes are modified or staged.
