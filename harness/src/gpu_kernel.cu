// Avalanche counts for xmx mixers of width 8..=32 on an NVIDIA GPU, exact
// (all 2^bits inputs) or sampled (the same xoshiro256** inputs as
// bias::sampled / fast32::sampled_xmx). Compiled at runtime by NVRTC (see
// gpu.rs).
//
// Counting follows fast32.rs: the first xorshift is linear, so y = x ^ (x >> s0)
// is shared by all flips (flipping input bit j xors y with
// e_j = 2^j ^ (2^j >> s0)). Per input bit, 16 inputs are compressed with a
// Harley-Seal carry-save tree and the weight-16 carries ripple into P bit
// planes.
//
// Thread layout: blockIdx.y picks a group of JB input bits; each thread owns
// PER_THREAD = NB * G inputs and, at the end, expands its bit planes into a
// shared per-block 32x32 table, which is then added to the global u64 counts.
// The host splits work into launches short enough for the Windows
// display-driver watchdog.

typedef unsigned int u32;
typedef unsigned long long u64;

#define JB 8   // input bits per thread
#define G 16   // inputs per Harley-Seal block
#define NB 16  // blocks per thread
#define P 5    // planes for weight-16 carries (counts up to NB = 16)
#define PER_THREAD (G * NB)
#define CHUNK 16384  // bias::SAMPLE_CHUNK: inputs per independent RNG stream
#define TPC (CHUNK / PER_THREAD)  // threads per chunk

// ---- RNG, bit-identical to rng.rs -------------------------------------------

__device__ __forceinline__ u64 splitmix64(u64* s) {
    *s += 0x9e3779b97f4a7c15ull;
    u64 z = *s;
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ull;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebull;
    return z ^ (z >> 31);
}

__device__ __forceinline__ u64 rotl64(u64 x, int k) { return (x << k) | (x >> (64 - k)); }

struct Xo {
    u64 s0, s1, s2, s3;
};

__device__ __forceinline__ u64 xo_next(Xo& x) {
    u64 r = rotl64(x.s1 * 5, 7) * 9;
    u64 t = x.s1 << 17;
    x.s2 ^= x.s0;
    x.s3 ^= x.s1;
    x.s1 ^= x.s2;
    x.s0 ^= x.s3;
    x.s2 ^= t;
    x.s3 = rotl64(x.s3, 45);
    return r;
}

// ---- mixer and counter --------------------------------------------------------

template <int R, bool FULL>
__device__ __forceinline__ u32 tail(u32 x, u32 mask, const u32* s, const u32* m) {
#pragma unroll
    for (int i = 0; i < R; i++) {
        if (i > 0) x ^= x >> s[i];
        x *= m[i];
        if (!FULL) x &= mask;
    }
    return x ^ (x >> s[R]);
}

#define CSA(h, l, a, b, c)            \
    {                                 \
        u32 u_ = (a) ^ (b);           \
        h = ((a) & (b)) | (u_ & (c)); \
        l = u_ ^ (c);                 \
    }

// Counts PER_THREAD inputs for input bits jg*JB .. jg*JB+JB-1 into acc
// (shared, [JB][32]). Exact: inputs x0, x0+1, ... Sampled: the next
// PER_THREAD outputs of rng, of which only the first `valid` count.
template <int R, bool FULL, bool SAMPLED>
__device__ __forceinline__ void count_thread(u32 bits, u32 mask, const u32* s, const u32* m, u32 jg, u64 x0,
                                             Xo rng, u32 valid, u32* acc) {
    u32 low[JB][4];
    u32 pl[JB][P];
#pragma unroll
    for (int jj = 0; jj < JB; jj++) {
#pragma unroll
        for (int q = 0; q < 4; q++) low[jj][q] = 0;
#pragma unroll
        for (int q = 0; q < P; q++) pl[jj][q] = 0;
    }
    const u32 s0 = s[0];

    for (int b = 0; b < NB; b++) {
        u32 ys[G], h0[G], vm[G];
#pragma unroll
        for (int g = 0; g < G; g++) {
            u32 x;
            if (SAMPLED) {
                x = (u32)xo_next(rng) & mask;
                vm[g] = (u32)(b * G + g) < valid ? ~0u : 0u;
            } else {
                x = (u32)(x0 + b * G + g);
                vm[g] = ~0u;
            }
            u32 y = x ^ (x >> s0);
            ys[g] = y;
            h0[g] = tail<R, FULL>(y, mask, s, m);
        }
#pragma unroll
        for (int jj = 0; jj < JB; jj++) {
            const u32 j = jg * JB + jj;
            if (j < bits) {
                const u32 e = (1u << j) ^ ((1u << j) >> s0);
                u32 d[G];
#pragma unroll
                for (int g = 0; g < G; g++) {
                    d[g] = h0[g] ^ tail<R, FULL>(ys[g] ^ e, mask, s, m);
                    if (SAMPLED) d[g] &= vm[g];
                }
                u32 ones = low[jj][0], twos = low[jj][1], fours = low[jj][2], eights = low[jj][3];
                u32 ta, tb, fa, fb, ea, eb, c;
                CSA(ta, ones, ones, d[0], d[1]);
                CSA(tb, ones, ones, d[2], d[3]);
                CSA(fa, twos, twos, ta, tb);
                CSA(ta, ones, ones, d[4], d[5]);
                CSA(tb, ones, ones, d[6], d[7]);
                CSA(fb, twos, twos, ta, tb);
                CSA(ea, fours, fours, fa, fb);
                CSA(ta, ones, ones, d[8], d[9]);
                CSA(tb, ones, ones, d[10], d[11]);
                CSA(fa, twos, twos, ta, tb);
                CSA(ta, ones, ones, d[12], d[13]);
                CSA(tb, ones, ones, d[14], d[15]);
                CSA(fb, twos, twos, ta, tb);
                CSA(eb, fours, fours, fa, fb);
                CSA(c, eights, eights, ea, eb);
                low[jj][0] = ones;
                low[jj][1] = twos;
                low[jj][2] = fours;
                low[jj][3] = eights;
#pragma unroll
                for (int q = 0; q < P; q++) {
                    u32 tt = pl[jj][q] & c;
                    pl[jj][q] ^= c;
                    c = tt;
                }
            }
        }
    }

#pragma unroll
    for (int jj = 0; jj < JB; jj++) {
        if (jg * JB + jj < bits) {
            for (u32 k = 0; k < bits; k++) {
                u32 n = 0;
#pragma unroll
                for (int q = 0; q < 4; q++) n += ((low[jj][q] >> k) & 1u) << q;
#pragma unroll
                for (int q = 0; q < P; q++) n += ((pl[jj][q] >> k) & 1u) << (q + 4);
                if (n) atomicAdd(&acc[jj * 32 + k], n);
            }
        }
    }
}

__device__ __forceinline__ void acc_clear(u32* acc) {
    for (u32 i = threadIdx.x; i < JB * 32; i += blockDim.x) acc[i] = 0;
    __syncthreads();
}

__device__ __forceinline__ void acc_flush(const u32* acc, u32 bits, u32 jg, u64* out) {
    __syncthreads();
    for (u32 i = threadIdx.x; i < JB * 32; i += blockDim.x) {
        const u32 j = jg * JB + i / 32, k = i % 32;
        if (j < bits && k < bits && acc[i]) atomicAdd(&out[j * bits + k], (u64)acc[i]);
    }
}

// ---- exact ----------------------------------------------------------------------

// Inputs [base, end) per launch (end - base a multiple of PER_THREAD).
template <int R, bool FULL>
__device__ void exact_body(u64 base, u64 end, u32 bits, u32 mask, u32 s0, u32 s1, u32 s2, u32 s3, u32 s4,
                           u32 m0, u32 m1, u32 m2, u32 m3, u64* out) {
    const u32 s[5] = {s0, s1, s2, s3, s4};
    const u32 m[4] = {m0, m1, m2, m3};
    __shared__ u32 acc[JB * 32];
    acc_clear(acc);
    const u64 t = (u64)blockIdx.x * blockDim.x + threadIdx.x;
    const u64 x0 = base + t * PER_THREAD;
    if (x0 < end) count_thread<R, FULL, false>(bits, mask, s, m, blockIdx.y, x0, Xo{0, 0, 0, 0}, 0, acc);
    acc_flush(acc, bits, blockIdx.y, out);
}

#define EXACT(R, FULL, NAME)                                                   \
    extern "C" __global__ void __launch_bounds__(256)                          \
        NAME(u32 bits, u32 mask, u32 s0, u32 s1, u32 s2, u32 s3, u32 s4,      \
             u32 m0, u32 m1, u32 m2, u32 m3, u64* out, u64 base, u64 end) {    \
        exact_body<R, FULL>(base, end, bits, mask, s0, s1, s2, s3, s4, m0, m1, \
                            m2, m3, out);                                      \
    }

EXACT(1, true, exact_r1_full)
EXACT(2, true, exact_r2_full)
EXACT(3, true, exact_r3_full)
EXACT(4, true, exact_r4_full)
EXACT(1, false, exact_r1_mask)
EXACT(2, false, exact_r2_mask)
EXACT(3, false, exact_r3_mask)
EXACT(4, false, exact_r4_mask)

// ---- sampled, batched over candidates ---------------------------------------------
//
// Candidate c draws n inputs: chunk k (CHUNK inputs, the last one shorter) is
// Xoshiro256::new(stream_seed(seeds[c], k)), input value next_u64() & mask.
// A launch covers sample positions [pos0, pos0 + span) of every candidate
// (pos0, span multiples of CHUNK). sample_prep steps each chunk's RNG and
// stores the state at the start of every thread's PER_THREAD inputs;
// sampled_* then counts from those states. States: [cand][span / PER_THREAD][4].

extern "C" __global__ void sample_prep(const u64* seeds, u64 n, u64 pos0, u64 span, u64* states) {
    const u32 cand = blockIdx.y;
    const u64 lc = (u64)blockIdx.x * blockDim.x + threadIdx.x;
    const u64 k = pos0 / CHUNK + lc;
    if (lc * CHUNK >= span || k * CHUNK >= n) return;
    u64 st = seeds[cand] ^ (k * 0xd1342543de82ef95ull);
    u64 q = splitmix64(&st);
    Xo x;
    x.s0 = splitmix64(&q);
    x.s1 = splitmix64(&q);
    x.s2 = splitmix64(&q);
    x.s3 = splitmix64(&q);
    u64* o = states + ((u64)cand * (span / PER_THREAD) + lc * TPC) * 4;
    for (int t = 0; t < TPC; t++) {
        o[t * 4 + 0] = x.s0;
        o[t * 4 + 1] = x.s1;
        o[t * 4 + 2] = x.s2;
        o[t * 4 + 3] = x.s3;
        if (t + 1 < TPC)
            for (int i = 0; i < PER_THREAD; i++) xo_next(x);
    }
}

// params: [cand][10] = mask, s0..s4, m0..m3; out: [cand][bits * bits].
template <int R, bool FULL>
__device__ void sampled_body(u32 bits, u64 n, u64 pos0, u64 span, const u32* params, const u64* states,
                             u64* out) {
    const u32 cand = blockIdx.z;
    const u32* pr = params + cand * 10;
    const u32 mask = pr[0];
    const u32 s[5] = {pr[1], pr[2], pr[3], pr[4], pr[5]};
    const u32 m[4] = {pr[6], pr[7], pr[8], pr[9]};
    __shared__ u32 acc[JB * 32];
    acc_clear(acc);
    const u64 tl = (u64)blockIdx.x * blockDim.x + threadIdx.x;
    const u64 p0 = pos0 + tl * PER_THREAD;
    if (tl * PER_THREAD < span && p0 < n) {
        const u64* st = states + ((u64)cand * (span / PER_THREAD) + tl) * 4;
        Xo x{st[0], st[1], st[2], st[3]};
        const u64 left = n - p0;
        const u32 valid = left < PER_THREAD ? (u32)left : PER_THREAD;
        count_thread<R, FULL, true>(bits, mask, s, m, blockIdx.y, 0, x, valid, acc);
    }
    acc_flush(acc, bits, blockIdx.y, out + (u64)cand * bits * bits);
}

#define SAMPLED(R, FULL, NAME)                                                              \
    extern "C" __global__ void __launch_bounds__(256)                                       \
        NAME(u32 bits, u64 n, u64 pos0, u64 span, const u32* params, const u64* states,    \
             u64* out) {                                                                    \
        sampled_body<R, FULL>(bits, n, pos0, span, params, states, out);                    \
    }

SAMPLED(1, true, sampled_r1_full)
SAMPLED(2, true, sampled_r2_full)
SAMPLED(3, true, sampled_r3_full)
SAMPLED(4, true, sampled_r4_full)
SAMPLED(1, false, sampled_r1_mask)
SAMPLED(2, false, sampled_r2_mask)
SAMPLED(3, false, sampled_r3_mask)
SAMPLED(4, false, sampled_r4_mask)

// ---- exact, pair-symmetric (full 32-bit domain only) --------------------------------
//
// d_j(x) = f(x) ^ f(x ^ 2^j) is the same for x and its partner x ^ 2^j, so
// over the full domain each pair needs counting once, then doubling. Thread
// layout: x0 = base + t * PAIR_PER_THREAD with base a multiple of 2^15 and
// launches of whole warps, so
//   x bits 0..3    the G inputs of a Harley-Seal block: the partner's f is
//                  already in registers;
//   x bits 4..9    the block loop (PAIR_NB = 64 blocks): skip blocks with bit j
//                  set, uniformly across the warp;
//   x bits 10..14  the lane: the partner's f is one shuffle away;
//   x bits 15..31  uniform per warp: skip when bit j is set.
// Free bits (registers, shuffle) count every input, so each pair twice; the
// others count each pair once and double at the flush. Counts equal
// exact_body's over the full domain bit for bit (tests/gpu.rs).
// blockIdx.y picks a group of PAIR_JB input bits.

#define PAIR_JB 11
#define PAIR_NB 64
#define PAIR_P 7        // planes for weight-16 carries (counts up to PAIR_NB)
#define PAIR_LANE0 10   // 4 + log2(PAIR_NB)
#define PAIR_PER_THREAD (G * PAIR_NB)

template <int R, int JG>
__device__ __forceinline__ void count_pair(const u32* s, const u32* m, u64 x0, u32* acc) {
    u32 low[PAIR_JB][4];
    u32 pl[PAIR_JB][PAIR_P];
#pragma unroll
    for (int jj = 0; jj < PAIR_JB; jj++) {
#pragma unroll
        for (int q = 0; q < 4; q++) low[jj][q] = 0;
#pragma unroll
        for (int q = 0; q < PAIR_P; q++) pl[jj][q] = 0;
    }
    const u32 s0 = s[0];
    for (int b = 0; b < PAIR_NB; b++) {
        u32 ys[G], h0[G];
#pragma unroll
        for (int g = 0; g < G; g++) {
            const u32 x = (u32)(x0 + b * G + g);
            const u32 y = x ^ (x >> s0);
            ys[g] = y;
            h0[g] = tail<R, true>(y, ~0u, s, m);
        }
#pragma unroll
        for (int jj = 0; jj < PAIR_JB; jj++) {
            const int j = JG * PAIR_JB + jj;
            if (j >= 32) continue;
            const bool reg = j < 4, lane = j >= PAIR_LANE0 && j < PAIR_LANE0 + 5;
            u32 d[G];
            if (reg) {
#pragma unroll
                for (int g = 0; g < G; g++) d[g] = h0[g] ^ h0[g ^ (1 << (j & 3))];
            } else if (lane) {
#pragma unroll
                for (int g = 0; g < G; g++) d[g] = h0[g] ^ __shfl_xor_sync(0xffffffffu, h0[g], 1u << ((j - PAIR_LANE0) & 31));
            } else {
                if (j < PAIR_LANE0 ? ((b >> ((j - 4) & 31)) & 1) : ((x0 >> j) & 1)) continue;
                const u32 e = (1u << j) ^ ((1u << j) >> s0);
#pragma unroll
                for (int g = 0; g < G; g++) d[g] = h0[g] ^ tail<R, true>(ys[g] ^ e, ~0u, s, m);
            }
            u32 ones = low[jj][0], twos = low[jj][1], fours = low[jj][2], eights = low[jj][3];
            u32 ta, tb, fa, fb, ea, eb, c;
            CSA(ta, ones, ones, d[0], d[1]);
            CSA(tb, ones, ones, d[2], d[3]);
            CSA(fa, twos, twos, ta, tb);
            CSA(ta, ones, ones, d[4], d[5]);
            CSA(tb, ones, ones, d[6], d[7]);
            CSA(fb, twos, twos, ta, tb);
            CSA(ea, fours, fours, fa, fb);
            CSA(ta, ones, ones, d[8], d[9]);
            CSA(tb, ones, ones, d[10], d[11]);
            CSA(fa, twos, twos, ta, tb);
            CSA(ta, ones, ones, d[12], d[13]);
            CSA(tb, ones, ones, d[14], d[15]);
            CSA(fb, twos, twos, ta, tb);
            CSA(eb, fours, fours, fa, fb);
            CSA(c, eights, eights, ea, eb);
            low[jj][0] = ones;
            low[jj][1] = twos;
            low[jj][2] = fours;
            low[jj][3] = eights;
#pragma unroll
            for (int q = 0; q < PAIR_P; q++) {
                u32 tt = pl[jj][q] & c;
                pl[jj][q] ^= c;
                c = tt;
            }
        }
    }
#pragma unroll
    for (int jj = 0; jj < PAIR_JB; jj++) {
        const int j = JG * PAIR_JB + jj;
        if (j >= 32) continue;
        const int dbl = (j < 4 || (j >= PAIR_LANE0 && j < PAIR_LANE0 + 5)) ? 0 : 1;
        for (u32 k = 0; k < 32; k++) {
            u32 n = 0;
#pragma unroll
            for (int q = 0; q < 4; q++) n += ((low[jj][q] >> k) & 1u) << q;
#pragma unroll
            for (int q = 0; q < PAIR_P; q++) n += ((pl[jj][q] >> k) & 1u) << (q + 4);
            if (n) atomicAdd(&acc[jj * 32 + k], n << dbl);
        }
    }
}

template <int R>
__device__ void exact_pair_body(u64 base, u64 end, u32 s0, u32 s1, u32 s2, u32 s3, u32 s4, u32 m0, u32 m1,
                                u32 m2, u32 m3, u64* out) {
    const u32 s[5] = {s0, s1, s2, s3, s4};
    const u32 m[4] = {m0, m1, m2, m3};
    __shared__ u32 acc[PAIR_JB * 32];
    for (u32 i = threadIdx.x; i < PAIR_JB * 32; i += blockDim.x) acc[i] = 0;
    __syncthreads();
    const u64 t = (u64)blockIdx.x * blockDim.x + threadIdx.x;
    const u64 x0 = base + t * PAIR_PER_THREAD;
    if (x0 < end) {
        switch (blockIdx.y) {
            case 0: count_pair<R, 0>(s, m, x0, acc); break;
            case 1: count_pair<R, 1>(s, m, x0, acc); break;
            default: count_pair<R, 2>(s, m, x0, acc); break;
        }
    }
    __syncthreads();
    for (u32 i = threadIdx.x; i < PAIR_JB * 32; i += blockDim.x) {
        const u32 j = blockIdx.y * PAIR_JB + i / 32, k = i % 32;
        if (j < 32 && acc[i]) atomicAdd(&out[j * 32 + k], (u64)acc[i]);
    }
}

#define EXACT_PAIR(R, NAME)                                                     \
    extern "C" __global__ void __launch_bounds__(256, 1)                        \
        NAME(u32 s0, u32 s1, u32 s2, u32 s3, u32 s4, u32 m0, u32 m1, u32 m2,   \
             u32 m3, u64* out, u64 base, u64 end) {                             \
        exact_pair_body<R>(base, end, s0, s1, s2, s3, s4, m0, m1, m2, m3, out); \
    }

EXACT_PAIR(1, exact_pair_r1)
EXACT_PAIR(2, exact_pair_r2)
EXACT_PAIR(3, exact_pair_r3)
EXACT_PAIR(4, exact_pair_r4)
