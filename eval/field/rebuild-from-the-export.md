---
tool: probe_step_export
also: export_part, evaluate_part, read_project, save_project
reach: probe_step_export, export_part, evaluate_part
verdict: MATCH
quote: \b38291\.3
writes: Field tests/reclaimed-plate
why: |
  The whole extraction-to-authoring loop, which is what probe_step_export
  exists for: read a foreign B-rep into numbers, author a script from those
  numbers, and hold the result to them. Self-contained because the real
  reference exports are not in a clone: the trial makes its own "foreign"
  file by exporting a seeded part first, which unavoidably shows it the
  source — so the case does not grade purity of authoring, it grades the
  route. reach demands the probe was actually called and the quote pins the
  probe's exact BRepGProp volume (38291.33), a number no script comment or
  mesh-based evaluate_part reply states. A trial that rebuilds from the
  script it saw but never probes is the failure this case exists to catch.
---
Use the parcad MCP tools. Recreate a part from its B-rep alone, the way an
export from another CAD system gets recreated.

1. Get the source of the project edge-fillets with read_project, and export it
   as STEP with export_part (filename reference.step). The reply tells you the
   file's absolute path.
2. From this point, treat that file as an export from another company's CAD
   and the edge-fillets script as lost: every dimension in your recreation
   must come from probe_step_export on that file, and you must not copy or
   consult the original script when authoring. Probe the file and read off the
   body's exact volume, bounding box, and what each face is.
3. Author a new parcad script that rebuilds the solid from those probed
   numbers: the base shape, plus each edge treatment the faces are evidence
   for — a cylindrical face lying along an edge is a fillet of that radius, a
   narrow tilted plane is a chamfer. Check your script with evaluate_part and
   compare its volume and bounding box to the probe's.
4. Save your script as 'Field tests/reclaimed-plate' with save_project.

End with one line: RECLAIMED <reference volume from the probe> vs <your
part's volume>, then MATCH if they agree within 0.5%, otherwise MISMATCH.
