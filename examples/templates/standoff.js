// A round standoff with a through bore, written as a template: another part
// takes one with importPart("templates/standoff", { height: 6 }) and gets this
// standoff at that height. Run on its own it builds with the defaults below.
const p = partParams({ od: 8, bore: 3.4, height: 10 });
return cylinder(p.od / 2, p.height).cut(cylinder(p.bore / 2, p.height + 1)).tag("standoff");
