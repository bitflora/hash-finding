/* Stream a 2-round 32-bit xmx mixer to stdout for PractRand (RNG_test stdin32).
 *
 *   cc -O2 -o feed practrand_feed.c
 *   ./feed M0 M1 MODE [R] | ./RNG_test stdin32 -tlmax 16GB -multithreaded
 *
 * Mixer: x ^= x>>16; x *= M0; x ^= x>>15; x *= M1; x ^= x>>15  (shifts 16/15/15)
 * Each output is mix(input(i)) for a 32-bit counter i; inputs are RRC-style
 * transforms (after Pelle Evensen), which expose weak mixers far sooner than
 * a plain counter:
 *   MODE 0  i                 plain counter
 *   MODE 1  rotl(i, R)        rotated counter (R = 0..31)
 *   MODE 2  reverse(i)        bit-reversed counter
 *   MODE 3  ~i                complemented counter
 *   MODE 4  i * 0x9e3779b9    Weyl-sequence (gamma) stride
 * The counter covers 2^32 inputs (16 GB of output) before it wraps, and a
 * bijection over a full period has no repeats, which PractRand will flag
 * near 16 GB for ANY mixer. So compare failure points below 16 GB, or treat
 * "passes to 8 GB" as the ceiling of this setup.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static uint32_t M0, M1;

static uint32_t mix(uint32_t x) {
    x ^= x >> 16; x *= M0;
    x ^= x >> 15; x *= M1;
    x ^= x >> 15;
    return x;
}

static uint32_t rev(uint32_t x) {
    x = ((x >> 1) & 0x55555555u) | ((x & 0x55555555u) << 1);
    x = ((x >> 2) & 0x33333333u) | ((x & 0x33333333u) << 2);
    x = ((x >> 4) & 0x0f0f0f0fu) | ((x & 0x0f0f0f0fu) << 4);
    x = ((x >> 8) & 0x00ff00ffu) | ((x & 0x00ff00ffu) << 8);
    return (x >> 16) | (x << 16);
}

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr, "usage: %s M0 M1 MODE [R]   (hex multipliers, e.g. 9c013335 335a2d97 2)\n", argv[0]);
        return 1;
    }
    M0 = (uint32_t)strtoul(argv[1], 0, 16);
    M1 = (uint32_t)strtoul(argv[2], 0, 16);
    int mode = atoi(argv[3]);
    unsigned r = argc > 4 ? (unsigned)atoi(argv[4]) & 31 : 0;
    static uint32_t buf[1 << 14];
    for (uint32_t i = 0;;) {
        for (int k = 0; k < (1 << 14); k++, i++) {
            uint32_t x = i;
            switch (mode) {
            case 1: x = r ? (i << r) | (i >> (32 - r)) : i; break;
            case 2: x = rev(i); break;
            case 3: x = ~i; break;
            case 4: x = i * 0x9e3779b9u; break;
            }
            buf[k] = mix(x);
        }
        if (fwrite(buf, sizeof buf, 1, stdout) != 1) return 0;
    }
}
