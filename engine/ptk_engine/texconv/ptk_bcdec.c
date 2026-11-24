/*
 * ptk-bcdec: decodes one level of BC-compressed blocks into RGBA pixels.
 *
 *   ptk-bcdec <bc1|bc2|bc3|bc7> <width> <height> <in.blocks> <out.rgba>
 *
 * The input holds ceil(width/4) * ceil(height/4) blocks, row by row (8 bytes
 * per block for BC1, 16 for the others), the way Godot stores a mip level.
 * The output is width * height * 4 bytes of RGBA, padding blocks cropped.
 * Part of Porting Toolkit (MIT); the decoding is bcdec's (MIT/Unlicense).
 */
#define BCDEC_IMPLEMENTATION
#include "bcdec.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fail(const char *message) {
    fprintf(stderr, "ptk-bcdec: %s\n", message);
    return 1;
}

int main(int argc, char **argv) {
    if (argc != 6) {
        return fail("usage: ptk-bcdec <bc1|bc2|bc3|bc7> <width> <height> <in> <out>");
    }
    const char *kind = argv[1];
    long width = strtol(argv[2], NULL, 10);
    long height = strtol(argv[3], NULL, 10);
    if (width < 1 || height < 1 || width > 65536 || height > 65536) {
        return fail("bad size");
    }
    int block_bytes = strcmp(kind, "bc1") == 0 ? 8 : 16;
    if (strcmp(kind, "bc1") && strcmp(kind, "bc2") && strcmp(kind, "bc3") && strcmp(kind, "bc7")) {
        return fail("unknown format");
    }
    long bw = (width + 3) / 4, bh = (height + 3) / 4;
    size_t in_size = (size_t)bw * (size_t)bh * (size_t)block_bytes;
    unsigned char *in = malloc(in_size);
    unsigned char *out = calloc((size_t)bw * 4 * (size_t)bh * 4 * 4, 1);
    if (!in || !out) {
        return fail("out of memory");
    }
    FILE *f = fopen(argv[4], "rb");
    if (!f || fread(in, 1, in_size, f) != in_size) {
        return fail("cannot read the blocks (file too short?)");
    }
    fclose(f);
    size_t pitch = (size_t)bw * 4 * 4; /* bytes per row of the padded image */
    for (long by = 0; by < bh; by++) {
        for (long bx = 0; bx < bw; bx++) {
            const unsigned char *block = in + ((size_t)by * bw + bx) * block_bytes;
            unsigned char *dst = out + (size_t)by * 4 * pitch + (size_t)bx * 16;
            if (kind[2] == '1') {
                bcdec_bc1(block, dst, (int)pitch);
            } else if (kind[2] == '2') {
                bcdec_bc2(block, dst, (int)pitch);
            } else if (kind[2] == '3') {
                bcdec_bc3(block, dst, (int)pitch);
            } else {
                bcdec_bc7(block, dst, (int)pitch);
            }
        }
    }
    FILE *o = fopen(argv[5], "wb");
    if (!o) {
        return fail("cannot write the output");
    }
    for (long y = 0; y < height; y++) {
        if (fwrite(out + (size_t)y * pitch, 1, (size_t)width * 4, o) != (size_t)width * 4) {
            return fail("write failed");
        }
    }
    fclose(o);
    free(in);
    free(out);
    return 0;
}
