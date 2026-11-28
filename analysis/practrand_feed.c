/* Stream a 32-bit xorshift/multiply mixer to stdout for PractRand (stdin32).
 *
 *   cc -O2 -o feed practrand_feed.c
 *   ./feed M0 M1 MODE [R] | ./RNG_test stdin32 -tlmax 512MB -multithreaded
 *   ./feed --spec "[17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]" MODE [R]
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
 * A bijection emits distinct words throughout the first 2^32 inputs (16 GiB).
 * PractRand can detect the resulting repeat deficit well before the wrap.
 */
#include <stdint.h>
#include <errno.h>
#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

struct mixer {
    unsigned rounds;
    unsigned shifts[4];
    uint32_t multipliers[3];
};

static uint32_t mix(uint32_t x, const struct mixer *mixer) {
    x ^= x >> mixer->shifts[0]; x *= mixer->multipliers[0];
    x ^= x >> mixer->shifts[1]; x *= mixer->multipliers[1];
    x ^= x >> mixer->shifts[2];
    if (mixer->rounds == 3) {
        x *= mixer->multipliers[2];
        x ^= x >> mixer->shifts[3];
    }
    return x;
}

static int parse_number(const char **input, int base, unsigned long max,
                        unsigned long *value) {
    const char *start = *input;
    while (isspace((unsigned char)*start)) start++;
    if (!isxdigit((unsigned char)*start)) return 0;
    char *end;
    errno = 0;
    *value = strtoul(start, &end, base);
    if (end == start || errno == ERANGE || *value > max) return 0;
    *input = end;
    return 1;
}

static int parse_spec(const char *input, struct mixer *mixer) {
    if (*input++ != '[') return 0;
    for (unsigned round = 0; round <= 3; round++) {
        unsigned long value;
        if (!parse_number(&input, 10, 31, &value) || !value) return 0;
        mixer->shifts[round] = (unsigned)value;
        if (*input != ']' && !isspace((unsigned char)*input)) return 0;
        while (isspace((unsigned char)*input)) input++;
        if (*input == ']') {
            mixer->rounds = round;
            return round >= 2 && input[1] == '\0';
        }
        if (round == 3 || !parse_number(&input, 16, UINT32_MAX, &value) ||
            !(value & 1) || !isspace((unsigned char)*input)) return 0;
        mixer->multipliers[round] = (uint32_t)value;
    }
    return 0;
}

static uint32_t rev(uint32_t x) {
    x = ((x >> 1) & 0x55555555u) | ((x & 0x55555555u) << 1);
    x = ((x >> 2) & 0x33333333u) | ((x & 0x33333333u) << 2);
    x = ((x >> 4) & 0x0f0f0f0fu) | ((x & 0x0f0f0f0fu) << 4);
    x = ((x >> 8) & 0x00ff00ffu) | ((x & 0x00ff00ffu) << 8);
    return (x >> 16) | (x << 16);
}

int main(int argc, char **argv) {
    if (argc < 4 || argc > 5) {
        fprintf(stderr, "usage: %s M0 M1 MODE [R] or %s --spec \"[S0 M0 S1 M1 S2 [M2 S3]]\" MODE [R]\n", argv[0], argv[0]);
        return 1;
    }
    struct mixer mixer = {2, {16, 15, 15}, {0}};
    if (strcmp(argv[1], "--spec") == 0) {
        if (!parse_spec(argv[2], &mixer)) {
            fprintf(stderr, "invalid mixer spec: %s\n", argv[2]);
            return 1;
        }
    } else {
        for (unsigned round = 0; round < 2; round++) {
            const char *input = argv[round + 1];
            unsigned long multiplier;
            if (!parse_number(&input, 16, UINT32_MAX, &multiplier) || *input || !(multiplier & 1)) {
                fprintf(stderr, "invalid multiplier: %s\n", argv[round + 1]);
                return 1;
            }
            mixer.multipliers[round] = (uint32_t)multiplier;
        }
    }
#ifdef _WIN32
    if (_setmode(_fileno(stdout), _O_BINARY) == -1) {
        perror("set stdout binary mode");
        return 1;
    }
#endif
    const char *mode_arg = argv[3];
    const char *rotation_arg = argc > 4 ? argv[4] : "0";
    unsigned long mode, r;
    if (!parse_number(&mode_arg, 10, 4, &mode) || *mode_arg ||
        !parse_number(&rotation_arg, 10, 31, &r) || *rotation_arg ||
        (argc > 4 && mode != 1)) {
        fprintf(stderr, "expected mode 0..4 and an optional rotation 0..31 for mode 1\n");
        return 1;
    }
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
            buf[k] = mix(x, &mixer);
        }
        if (fwrite(buf, sizeof buf, 1, stdout) != 1) return 0;
    }
}
