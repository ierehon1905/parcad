// An L-shaped block, 20 × 20 with 5 mm legs and 10 tall, shelled by 1 with
// sharp corners: the cavity is the L stepped in by 1, its inside corner kept
// square, so the wall is 18 × 3 + 3 × 15 = 99 mm² of cavity over 8 mm:
// 1750 - 792 = 958.000 mm3. Rounded corners would leave 956.2.
return extrude([[0, 0], [20, 0], [20, 5], [5, 5], [5, 20], [0, 20]], 10)
  .shell(1, { corners: "sharp" })
  .tag("block");
