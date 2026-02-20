/*
 * Shannon entropy calculator — C implementation for FFI demonstration.
 * Called from Rust via FFI to show cross-language integration.
 */
#include <math.h>
#include <stddef.h>
#include <stdint.h>

double compute_shannon_entropy(const uint8_t *data, size_t len) {
    if (data == NULL || len == 0) {
        return 0.0;
    }

    unsigned long freq[256] = {0};
    for (size_t i = 0; i < len; i++) {
        freq[data[i]]++;
    }

    double entropy = 0.0;
    double log2 = log(2.0);
    for (int i = 0; i < 256; i++) {
        if (freq[i] > 0) {
            double p = (double)freq[i] / (double)len;
            entropy -= p * (log(p) / log2);
        }
    }

    return entropy;
}
