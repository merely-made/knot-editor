-- One row per fold, carrying its source span, plus a trailing plain note.
local out = {}
for _, fold in ipairs(folds()) do
  out[#out + 1] = row(fold.kind, fold.span)
end
out[#out + 1] = note("folds: " .. #out)
return out
