// Union and difference create section edges the fillet builder can work on; the
// bindings report none for an intersection. The correct answer is to refuse,
// not to return an unblended intersection that looks close enough.
return intersect(box(20, 20, 20), sphere(13), { blend: 3 });
