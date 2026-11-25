// Exact avalanche counts for xmx mixers of width 8..=32 on an NVIDIA GPU.
// Compiled at runtime by NVRTC (see gpu.rs). Same counting scheme as
// fast32.rs: the first xorshift is linear, so y = x ^ (x >> s0) is shared by
// all flips (flipping input bit j xors y with e_j = 2^j ^ (2^j >> s0)); per
// input bit, 16 consecutive inputs are compressed with a Harley-Seal
// carry-save tree and the weight-16 carries ripple into P bit planes.
//
// Thread layout: blockIdx.y picks a group of JB input bits; each thread owns
// NB blocks of G consecutive inputs (G * NB = 256) and, at the end, expands
// its bit planes into a shared per-block 32x32 table, which is then added to
// the global u64 counts. Inputs [base, end) are covered per launch (end - base
// a multiple of 256), so the host can split the 2^32 inputs into launches
// short enough for the Windows display-driver watchdog.

typedef unsigned int u32;
typedef unsigned long long u64;

#define JB 8   // input bits per thread
#define G 16   // inputs per Harley-Seal block
#define NB 16  // blocks per thread
#define P 5    // planes for weight-16 carries (counts up to NB = 16)

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

#define CSA(h, l, a, b, c)                 \
    {                                      \
        u32 u_ = (a) ^ (b);                \
        h = ((a) & (b)) | (u_ & (c));      \
        l = u_ ^ (c);                      \
    }

template <int R, bool FULL>
__device__ void exact_body(u64 base, u64 end, u32 bits, u32 mask, u32 s0, u32 s1, u32 s2, u32 s3, u32 s4,
                           u32 m0, u32 m1, u32 m2, u32 m3, u64* out) {
    const u32 s[5] = {s0, s1, s2, s3, s4};
    const u32 m[4] = {m0, m1, m2, m3};
    __shared__ u32 acc[JB * 32];
    for (u32 i = threadIdx.x; i < JB * 32; i += blockDim.x) acc[i] = 0;
    __syncthreads();

    const u32 jg = blockIdx.y;
    const u64 t = (u64)blockIdx.x * blockDim.x + threadIdx.x;
    const u64 x0 = base + t * (G * NB);
    if (x0 < end) {
        u32 low[JB][4];
        u32 pl[JB][P];
#pragma unroll
        for (int jj = 0; jj < JB; jj++) {
#pragma unroll
            for (int q = 0; q < 4; q++) low[jj][q] = 0;
#pragma unroll
            for (int q = 0; q < P; q++) pl[jj][q] = 0;
        }

        for (int b = 0; b < NB; b++) {
            u32 ys[G], h0[G];
#pragma unroll
            for (int g = 0; g < G; g++) {
                u32 x = (u32)(x0 + b * G + g);
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
                    for (int g = 0; g < G; g++) d[g] = h0[g] ^ tail<R, FULL>(ys[g] ^ e, mask, s, m);
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
                    low[jj][0] = ones; low[jj][1] = twos; low[jj][2] = fours; low[jj][3] = eights;
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
    __syncthreads();
    for (u32 i = threadIdx.x; i < JB * 32; i += blockDim.x) {
        const u32 j = jg * JB + i / 32, k = i % 32;
        if (j < bits && k < bits && acc[i]) atomicAdd(&out[j * bits + k], (u64)acc[i]);
    }
}

#define ENTRY(R, FULL, NAME)                                                   \
    extern "C" __global__ void __launch_bounds__(256)                          \
        NAME(u32 bits, u32 mask, u32 s0, u32 s1, u32 s2, u32 s3, u32 s4,      \
             u32 m0, u32 m1, u32 m2, u32 m3, u64* out, u64 base, u64 end) {    \
        exact_body<R, FULL>(base, end, bits, mask, s0, s1, s2, s3, s4, m0, m1, \
                            m2, m3, out);                                      \
    }

ENTRY(1, true, exact_r1_full)
ENTRY(2, true, exact_r2_full)
ENTRY(3, true, exact_r3_full)
ENTRY(4, true, exact_r4_full)
ENTRY(1, false, exact_r1_mask)
ENTRY(2, false, exact_r2_mask)
ENTRY(3, false, exact_r3_mask)
ENTRY(4, false, exact_r4_mask)
