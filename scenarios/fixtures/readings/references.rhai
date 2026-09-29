// A bounded visual receipt for the links the source actually declares.
let out = [];
for link in links() {
    if link.has_span {
        out.push(row(link.text + " · " + link.target, link.span));
    }
}
out
