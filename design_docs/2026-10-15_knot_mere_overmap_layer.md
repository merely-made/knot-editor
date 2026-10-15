# Knot's resident mere as an overmap layer

Status: implemented; Knot checks and the Djinn round trip pass (below). Native
foreground qualification of the layer in a running Knot is not done.

Shared source: Mere `19573b513d3809290973a0342b530091e8ca7ba1` (the application
door, Mere stack lift plan P1), Genet `ae3d9f5d`, Murm `3d9e799f`, Netrender
`f4f9c0d8`, Classic provider `276b4664`, Dramatis `0234e7ee`.

## Ruling

Mere's `mere-view` grew archive controls (`MereViewModel::archive`,
`MereViewRequest::Archive`) and `hide_graph` after the revision Knot pinned.
Mark ruled on 2026-10-15 that Knot forwards archive requests to Djinn, and then
gave the shape: "catalog, route as overmap". Knot's file-catalog graph stays the
graph. When Djinn holds a mere, that mere's route shows as an overmap layer
above it: sessions as containers, archives on them.

## Contract

- **The graph is the catalog.** `MereViewModel::graph` is always Knot's catalog
  reading. The route's graph projection is read only so `mere-overmap`'s
  `route::fill` can mark the attached session; it is never displayed. Where the
  two disagree, the catalog wins.
- **The layer is the route's.** `sessions`, `can_mint` and `archive` come from
  Djinn's `mere/<domain>` route, read through the first-party door as
  `knot-editor` (knot-editor `mere_overlay`, feature `mere-overlay`). The
  mapping is `mere-overmap`'s (`route::fill`, `route::fill_archive`,
  `route::intent`, `route::archive_intent`); Knot copies none of it. The
  archive controls show only when the route advertises them.
- **Requests split by owner.** Session steps, Mint and `Archive(Save | Open |
  Compose)` go to the route. Activate, Layout and host actions stay with
  `carry_graph_request`. The route's refusal reason becomes the view's notice.
- **Without Djinn,** there are no sessions and no archive, and `hide_graph`
  is false. A session or archive request is declined with the view's notice
  ("Archives come from a resident mere; ...").
- **Launch.** `--mere-domain <domain>` names the mere, beside `--mere-root`
  and `--mere-persona`, which it requires. The route's grant is the owner's:
  Djinn admits `knot-editor` on `mere/<domain>` only where reservoir access
  allows it. The worker reads once on open and again after every request; it
  never blocks the UI thread, and a failure withdraws the layer with a notice.

## Checks (2026-10-15)

- `cargo check --workspace --locked --all-targets`: pass.
- `TMPDIR=/tmp cargo test --locked -p knot-editor --features mere-overlay`:
  pass, including the three `mere_overlay` unit tests (the layer is empty
  without a resident; a reading adds sessions and archive and keeps the
  catalog graph; requests split by owner).
- `TMPDIR=/tmp cargo test --locked -p knot-desktop --lib --bins --test
  scenario_lane --test font_roles --test typography_fonts`: pass, including the
  new `--mere-domain` parsing assertions.
- Round trip, in Mere's Djinn (`resident_mere`
  `knot_overlay_saves_and_opens_an_archive_through_the_door`): a Djinn fixture
  serves `mere/divination` behind the app broker; `MereOverlay` opens it as
  `knot-editor`, finds the attached session in `archive.save_sessions`, saves
  it, reads the codicil back, opens it, and has an unadvertised Compose
  declined before it reaches the route.

Not done: a native capture of the layer in a running Knot with Djinn, and any
choice of which mere a default launch should overlay.
