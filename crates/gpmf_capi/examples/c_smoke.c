/*
 * c_smoke.c - manual end-to-end check that the jgpmf static library + header
 * link and run from a real C compiler.
 *
 * Reads a GPMF sample payload from a file given on argv[1] and prints the
 * sample counts for each stream plus the first GPS9 fix if any.
 *
 * Build (run from the workspace root after `cargo build -p gpmf_capi --release`):
 *
 *   Windows (MSVC):
 *     cl /nologo /I crates\gpmf_capi\include ^
 *        crates\gpmf_capi\examples\c_smoke.c ^
 *        target\release\jgpmf_capi.lib ^
 *        /link Ws2_32.lib Userenv.lib ntdll.lib Bcrypt.lib Advapi32.lib
 *
 *   Linux / macOS:
 *     cc -I crates/gpmf_capi/include \
 *        crates/gpmf_capi/examples/c_smoke.c \
 *        target/release/libjgpmf_capi.a \
 *        -o c_smoke -lpthread -ldl -lm
 *
 * Run:
 *     ./c_smoke crates/gpmf_parser/test_files/sample_60.bin
 */

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

#include "jgpmf_capi.h"

static int read_whole_file(const char *path, uint8_t **out_bytes, size_t *out_len) {
    FILE *fp = fopen(path, "rb");
    if (!fp) return -1;
    if (fseek(fp, 0, SEEK_END) != 0) { fclose(fp); return -1; }
    long n = ftell(fp);
    if (n < 0) { fclose(fp); return -1; }
    if (fseek(fp, 0, SEEK_SET) != 0) { fclose(fp); return -1; }
    uint8_t *buf = (uint8_t *)malloc((size_t)n);
    if (!buf) { fclose(fp); return -1; }
    size_t got = fread(buf, 1, (size_t)n, fp);
    fclose(fp);
    if (got != (size_t)n) { free(buf); return -1; }
    *out_bytes = buf;
    *out_len = (size_t)n;
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <gpmf_sample.bin>\n", argv[0]);
        return 2;
    }

    uint32_t major, minor, patch;
    jgpmf_version(&major, &minor, &patch);
    printf("jgpmf library version: %u.%u.%u\n", major, minor, patch);

    uint8_t *bytes = NULL;
    size_t len = 0;
    if (read_whole_file(argv[1], &bytes, &len) != 0) {
        fprintf(stderr, "failed to read %s\n", argv[1]);
        return 1;
    }
    printf("read %zu bytes from %s\n", len, argv[1]);

    JgpmfSample *sample = NULL;
    JgpmfStatus st = jgpmf_sample_parse(bytes, len, &sample);
    free(bytes);
    if (st != JGPMF_OK) {
        fprintf(stderr, "jgpmf_sample_parse failed: status=%d\n", (int)st);
        return 1;
    }

    const JgpmfVec3 *vec;
    size_t count;

    if (jgpmf_sample_accl(sample, &vec, &count) == JGPMF_OK) {
        printf("ACCL: %zu samples", count);
        if (count > 0) printf(" first=(%.3f, %.3f, %.3f)", vec[0].x, vec[0].y, vec[0].z);
        putchar('\n');
    }
    if (jgpmf_sample_gyro(sample, &vec, &count) == JGPMF_OK) {
        printf("GYRO: %zu samples\n", count);
    }
    if (jgpmf_sample_grav(sample, &vec, &count) == JGPMF_OK) {
        printf("GRAV: %zu samples\n", count);
    }

    const JgpmfQuat *quat;
    if (jgpmf_sample_cori(sample, &quat, &count) == JGPMF_OK) {
        printf("CORI: %zu samples\n", count);
    }
    if (jgpmf_sample_iori(sample, &quat, &count) == JGPMF_OK) {
        printf("IORI: %zu samples\n", count);
    }

    JgpmfGps9 gps;
    st = jgpmf_sample_get_gps9(sample, &gps);
    if (st == JGPMF_OK) {
        printf("GPS9: fix=%u lat=%.6f lon=%.6f alt=%.1fm speed2d=%.2f m/s\n",
               gps.fix, gps.latitude, gps.longitude, gps.altitude, gps.speed_2d);
    } else if (st == JGPMF_ERR_NO_GPS9) {
        printf("GPS9: no fix in this sample\n");
    } else {
        printf("GPS9: error status=%d\n", (int)st);
    }

    jgpmf_sample_free(sample);
    return 0;
}
