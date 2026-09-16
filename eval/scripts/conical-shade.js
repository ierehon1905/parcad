// A lampshade in closed form: a frustum shell open at both ends, its inner
// surface the outer one moved 1.6 mm in along the radius. The slope is
// 10 in 40, so the wall between the two surfaces is 1.6 * 40 / sqrt(40^2 + 10^2)
// = 1.5522 mm, and the open rims are where a thickness sweep reads less.
return revolve([[28.4, 0], [30, 0], [20, 40], [18.4, 40]]).tag("shade");
