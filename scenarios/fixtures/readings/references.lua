-- A bounded visual receipt for the links the source actually declares.
local out = {}
for _, link in ipairs(links()) do
  if link.span then
    out[#out + 1] = row(link.text .. " · " .. link.target, link.span)
  end
end
return out
