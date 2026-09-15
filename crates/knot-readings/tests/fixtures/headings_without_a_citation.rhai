// Sections whose body contains no `rel=cites` link.
let cited = [];
for link in links() {
    if link.rel == "cites" && link.has_span {
        cited.push(link.span.start);
    }
}
let out = [];
for section in sections() {
    let found = false;
    for at in cited {
        if at >= section.body.start && at < section.body.end {
            found = true;
        }
    }
    if !found {
        out.push(row(section.label, section.heading));
    }
}
out
