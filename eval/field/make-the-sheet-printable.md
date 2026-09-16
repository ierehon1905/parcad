---
tool: export_part
also: evaluate_part, read_docs
reach: export_part
input: \.thicken\(\s*1\.4\b
verdict: VOLUME\s*[=:]\s*3\s?971\.99\d*\s*mm
trap: VOLUME\s*[=:]\s*0(\.0*)?\s*mm
quote: \b3\s?971\.99\d*\b
why: |
  Whether a refusal that names its fix is acted on, in the surface mode. The
  script is an open lofted sheet, a surface: `export_part` with STL refuses
  it, saying STL describes closed solids and naming `.thicken(t)`. The route
  is to add `.thicken(1.4)` to the script — `input` is what shows it — and
  export again, and the export's own `measured.volume_mm3`, 3971.99, is the
  answer (3971.80 before OCCT patch 0003 re-meshed it; every trial quoted the
  new number); nothing in the script states it, since the sheet's area is not
  written anywhere. The trap is a volume of zero, read off a surface. A trial
  that builds the wall some other way (an extrude of an offset curve) has not
  used the surface mode and cannot land on the quote.
---
Use the parcad MCP tools.

I drew this curved sheet and need it as an STL my printer can print, with
walls 1.4 mm thick:

    const arc = (r, n) => Array.from({ length: n }, (_, i) => {
      const a = (Math.PI / 3) * (i / (n - 1)) - Math.PI / 6;
      return [r * Math.cos(a), r * Math.sin(a)];
    });
    return surfaceLoft([
      { z: 0, curve: [{ fit: arc(40, 12), tolerance: 0.01 }] },
      { z: 30, curve: [{ fit: arc(46, 12), tolerance: 0.01 }] },
      { z: 60, curve: [{ fit: arc(38, 12), tolerance: 0.01 }] },
    ], { smooth: true });

Export it as an STL and tell me the volume of what the file holds.

Rules: the volume must be the one a parcad tool measured. End with one line in
exactly this form:

VOLUME = <volume> mm³
