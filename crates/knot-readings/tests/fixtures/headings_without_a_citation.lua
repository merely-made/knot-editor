-- Sections whose body contains no `rel=cites` link.
local cited = {}
for _, link in ipairs(links()) do
  if link.rel == "cites" and link.has_span then
    cited[#cited + 1] = link.span.start
  end
end
local out = {}
for _, section in ipairs(sections()) do
  local found = false
  for _, at in ipairs(cited) do
    if at >= section.body.start and at < section.body.stop then
      found = true
    end
  end
  if not found then
    out[#out + 1] = row(section.label, section.heading)
  end
end
return out
