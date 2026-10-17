/*
** Creëer: x^y the same on every platform (Lua's `^`, through luai_numpow in luaconf.h).
**
** A port of the Rust `libm` crate's pow (itself FreeBSD's e_pow.c), so that Lua's `^` and
** the game's own maths (`creeer_math::det::pow`) agree bit for bit. Only IEEE double
** arithmetic, compiled without contraction into fused multiply-adds.
**
** Copyright (C) 2004 by Sun Microsystems, Inc. All rights reserved.
** Permission to use, copy, modify, and distribute this software is freely granted,
** provided that this notice is preserved.
*/

#include <math.h>
#include <stdint.h>
#include <string.h>

static uint64_t bits(double x) {
  uint64_t u;
  memcpy(&u, &x, sizeof u);
  return u;
}

static double from_bits(uint64_t u) {
  double x;
  memcpy(&x, &u, sizeof x);
  return x;
}

static uint32_t high_word(double x) { return (uint32_t)(bits(x) >> 32); }

static double with_low_word(double x, uint32_t lo) {
  return from_bits((bits(x) & 0xffffffff00000000ull) | lo);
}

static double with_high_word(double x, uint32_t hi) {
  return from_bits((bits(x) & 0x00000000ffffffffull) | ((uint64_t)hi << 32));
}

static const double BP[2] = {1.0, 1.5};
static const double DP_H[2] = {0.0, 5.84962487220764160156e-01};
static const double DP_L[2] = {0.0, 1.35003920212974897128e-08};
static const double TWO53 = 9007199254740992.0;
static const double HUGE_ = 1.0e300;
static const double TINY = 1.0e-300;
static const double L1 = 5.99999999999994648725e-01;
static const double L2 = 4.28571428578550184252e-01;
static const double L3 = 3.33333329818377432918e-01;
static const double L4 = 2.72728123808534006489e-01;
static const double L5 = 2.30660745775561754067e-01;
static const double L6 = 2.06975017800338417784e-01;
static const double P1 = 1.66666666666666019037e-01;
static const double P2 = -2.77777777770155933842e-03;
static const double P3 = 6.61375632143793436117e-05;
static const double P4 = -1.65339022054652515390e-06;
static const double P5 = 4.13813679705723846039e-08;
static const double LG2 = 6.93147180559945286227e-01;
static const double LG2_H = 6.93147182464599609375e-01;
static const double LG2_L = -1.90465429995776804525e-09;
static const double OVT = 8.0085662595372944372e-017;
static const double CP = 9.61796693925975554329e-01;
static const double CP_H = 9.61796700954437255859e-01;
static const double CP_L = -7.02846165095275826516e-09;
static const double IVLN2 = 1.44269504088896338700e+00;
static const double IVLN2_H = 1.44269502162933349609e+00;
static const double IVLN2_L = 1.92596299112661746887e-08;

double creeer_pow(double x, double y) {
  double t1, t2;
  int32_t hx = (int32_t)(bits(x) >> 32);
  uint32_t lx = (uint32_t)bits(x);
  int32_t hy = (int32_t)(bits(y) >> 32);
  uint32_t ly = (uint32_t)bits(y);
  int32_t ix = hx & 0x7fffffff;
  int32_t iy = hy & 0x7fffffff;
  int32_t yisint = 0, k, j;
  double ax, s, y1, p_l, p_h, z, t, u, v, w, r;
  int32_t i, n;

  if (((uint32_t)iy | ly) == 0) return 1.0;
  if (hx == 0x3ff00000 && lx == 0) return 1.0;
  if (ix > 0x7ff00000 || (ix == 0x7ff00000 && lx != 0) || iy > 0x7ff00000 ||
      (iy == 0x7ff00000 && ly != 0))
    return x + y;

  if (hx < 0) {
    if (iy >= 0x43400000) {
      yisint = 2;
    } else if (iy >= 0x3ff00000) {
      k = (iy >> 20) - 0x3ff;
      if (k > 20) {
        j = (int32_t)(ly >> (52 - k));
        if ((int32_t)((uint32_t)j << (52 - k)) == (int32_t)ly) yisint = 2 - (j & 1);
      } else if (ly == 0) {
        j = iy >> (20 - k);
        if ((j << (20 - k)) == iy) yisint = 2 - (j & 1);
      }
    }
  }

  if (ly == 0) {
    if (iy == 0x7ff00000) {
      if (((ix - 0x3ff00000) | (int32_t)lx) == 0) return 1.0;
      if (ix >= 0x3ff00000) return hy >= 0 ? y : 0.0;
      return hy >= 0 ? 0.0 : -y;
    }
    if (iy == 0x3ff00000) return hy >= 0 ? x : 1.0 / x;
    if (hy == 0x40000000) return x * x;
    if (hy == 0x3fe00000 && hx >= 0) return sqrt(x);
  }

  ax = fabs(x);
  if (lx == 0 && (ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000)) {
    z = ax;
    if (hy < 0) z = 1.0 / z;
    if (hx < 0) {
      if (((ix - 0x3ff00000) | yisint) == 0)
        z = (z - z) / (z - z);
      else if (yisint == 1)
        z = -z;
    }
    return z;
  }

  s = 1.0;
  if (hx < 0) {
    if (yisint == 0) return (x - x) / (x - x);
    if (yisint == 1) s = -1.0;
  }

  if (iy > 0x41e00000) {
    if (iy > 0x43f00000) {
      if (ix <= 0x3fefffff) return hy < 0 ? HUGE_ * HUGE_ : TINY * TINY;
      if (ix >= 0x3ff00000) return hy > 0 ? HUGE_ * HUGE_ : TINY * TINY;
    }
    if (ix < 0x3fefffff) return hy < 0 ? s * HUGE_ * HUGE_ : s * TINY * TINY;
    if (ix > 0x3ff00000) return hy > 0 ? s * HUGE_ * HUGE_ : s * TINY * TINY;
    t = ax - 1.0;
    w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
    u = IVLN2_H * t;
    v = t * IVLN2_L - w * IVLN2;
    t1 = with_low_word(u + v, 0);
    t2 = v - (t1 - u);
  } else {
    double ss, s2, s_h, s_l, t_h, t_l, z_h, z_l, pp_h, pp_l;
    n = 0;
    if (ix < 0x00100000) {
      ax *= TWO53;
      n -= 53;
      ix = (int32_t)high_word(ax);
    }
    n += (ix >> 20) - 0x3ff;
    j = ix & 0x000fffff;
    ix = j | 0x3ff00000;
    if (j <= 0x3988E) {
      k = 0;
    } else if (j < 0xBB67A) {
      k = 1;
    } else {
      k = 0;
      n += 1;
      ix -= 0x00100000;
    }
    ax = with_high_word(ax, (uint32_t)ix);

    u = ax - BP[k];
    v = 1.0 / (ax + BP[k]);
    ss = u * v;
    s_h = with_low_word(ss, 0);
    t_h = with_high_word(0.0, (((uint32_t)ix >> 1) | 0x20000000) + 0x00080000 + ((uint32_t)k << 18));
    t_l = ax - (t_h - BP[k]);
    s_l = v * ((u - s_h * t_h) - s_h * t_l);

    s2 = ss * ss;
    r = s2 * s2 * (L1 + s2 * (L2 + s2 * (L3 + s2 * (L4 + s2 * (L5 + s2 * L6)))));
    r += s_l * (s_h + ss);
    s2 = s_h * s_h;
    t_h = with_low_word(3.0 + s2 + r, 0);
    t_l = r - ((t_h - 3.0) - s2);

    u = s_h * t_h;
    v = s_l * t_h + t_l * ss;

    pp_h = with_low_word(u + v, 0);
    pp_l = v - (pp_h - u);
    z_h = CP_H * pp_h;
    z_l = CP_L * pp_h + pp_l * CP + DP_L[k];

    t = (double)n;
    t1 = with_low_word(((z_h + z_l) + DP_H[k]) + t, 0);
    t2 = z_l - (((t1 - t) - DP_H[k]) - z_h);
  }

  y1 = with_low_word(y, 0);
  p_l = (y - y1) * t1 + y * t2;
  p_h = y1 * t1;
  z = p_l + p_h;
  j = (int32_t)(bits(z) >> 32);
  i = (int32_t)bits(z);

  if (j >= 0x40900000) {
    if (((j - 0x40900000) | i) != 0) return s * HUGE_ * HUGE_;
    if (p_l + OVT > z - p_h) return s * HUGE_ * HUGE_;
  } else if ((j & 0x7fffffff) >= 0x4090cc00) {
    if ((((uint32_t)j - 0xc090cc00u) | (uint32_t)i) != 0) return s * TINY * TINY;
    if (p_l <= z - p_h) return s * TINY * TINY;
  }

  i = j & 0x7fffffff;
  k = (i >> 20) - 0x3ff;
  n = 0;
  if (i > 0x3fe00000) {
    n = j + (0x00100000 >> (k + 1));
    k = ((n & 0x7fffffff) >> 20) - 0x3ff;
    t = with_high_word(0.0, (uint32_t)(n & ~(0x000fffff >> k)));
    n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
    if (j < 0) n = -n;
    p_h -= t;
  }

  t = with_low_word(p_l + p_h, 0);
  u = t * LG2_H;
  v = (p_l - (t - p_h)) * LG2 + t * LG2_L;
  z = u + v;
  w = v - (z - u);
  t = z * z;
  t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
  r = (z * t1) / (t1 - 2.0) - (w + z * w);
  z = 1.0 - (r - z);
  j = (int32_t)high_word(z);
  j += (int32_t)((uint32_t)n << 20);

  if ((j >> 20) <= 0)
    z = ldexp(z, n);
  else
    z = with_high_word(z, (uint32_t)j);

  return s * z;
}
