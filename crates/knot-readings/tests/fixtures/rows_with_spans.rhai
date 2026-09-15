// One row per fold, carrying its source span, plus a trailing plain note.
let out = [];
for fold in folds() {
    out.push(row(fold.kind, fold.span));
}
out.push(note("folds: " + out.len()));
out
