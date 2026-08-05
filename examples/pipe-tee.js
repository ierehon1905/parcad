// A socket-weld pipe tee for 1" schedule 40 pipe.
//
// The pipe it joins is 33.4 OD, 3.38 wall, so the bore is 26.64. The fitting
// body is deliberately fatter than the pipe: its sockets have to swallow the
// pipe OD and still leave a wall, which is why the body diameters are not
// `pipeOd`. The run and branch bodies differ because a blended union of two
// equal-radius cylinders crossing at 90° aborts inside OCCT at this size —
// see docs/DSL_GAPS.md; unequal radii is also what a real fitting looks like. A
// socket cut wider than the body would not be a counterbore at all — it would
// saw the end off, and the reported bounding box is where that shows up.
//
// Two bodies of revolution crossing at 90° is the shape booleans are best at,
// and the reason this is a good part to test a kernel with: the branch
// intersection curve is a genuine 3D curve, not a circle, outside and in.

const pipeOd = 33.4;
const runOd = 48;      // fitting body around the run
const branchOd = 42;   // fitting body around the branch
const bore = 26.64;
const run = 100;      // end to end along X
const branch = 55;    // centre to branch face along Z
const socketDia = 33.9;  // slip fit over the pipe OD
const socketDepth = 12;

const runBody = cylinder(runOd / 2, run).rotate("y", 90).tag("run");

const branchBody = cylinder(branchOd / 2, branch)
  .at(0, 0, branch / 2)
  .tag("branch");

// The blend at the crotch is what a cast or forged fitting actually has, and
// it is structurally the point: the bare intersection of two cylinders is a
// stress concentration exactly where the pressure load is highest.
const body = union(runBody, branchBody, { blend: 2 }).tag("body");

// One bore through the run, one down the branch. They meet inside, so the
// hollow is a single connected volume — as it must be for a fitting.
const runBore = cylinder(bore / 2, run * 1.2).rotate("y", 90);
const branchBore = cylinder(bore / 2, branch * 1.2).at(0, 0, branch / 2);

// Sockets: a counterbore at each of the three ends that the pipe slips into,
// stopping on a shoulder. Modelled overlength outward for the same reason
// every other cutter here is.
const socket = cylinder(socketDia / 2, socketDepth * 2);
const sockets = union(
  socket.rotate("y", 90).at(run / 2, 0, 0),
  socket.rotate("y", 90).at(-run / 2, 0, 0),
  socket.at(0, 0, branch),
);

const bored = body.cut(runBore, branchBore, sockets).tag("bored");

// Break the branch's socket mouth, the rim a pipe is pushed into. Selecting on
// position rather than face normal is deliberate: a rim also borders its own
// cylindrical wall, and on this part those walls point in every direction.
return bored
  .edges({ generatedBy: "bored", curve: "circle", role: "hole", at: { z: "max" } })
  .expect({ count: 1 })
  .chamfer(1.5)
  .tag("branch_socket_lead_in");
