/*
 * C side of piirit-video: plain types for Rust.
 *
 * FFmpeg/x264 struct layouts vary by release: only these numbers cross;
 * src/lib.rs mirrors field for field.
 */
#ifndef PIIRIT_RECODE_H
#define PIIRIT_RECODE_H

#include <stddef.h>
#include <stdint.h>

/* Decodable video codecs, or "other". */
enum piirit_codec {
    PIIRIT_CODEC_NONE = 0,
    PIIRIT_CODEC_H264 = 1,
    PIIRIT_CODEC_HEVC = 2,
    PIIRIT_CODEC_OTHER = 3,
};

/* File contents from header, nothing decoded. */
struct piirit_probe {
    int64_t duration_ms;
    /* Picture as stored, before display matrix. */
    int32_t width;
    int32_t height;
    /* Display matrix turn, clockwise: 0, 90, 180, 270. */
    int32_t rotation;
    int32_t video_codec;
    int64_t video_bit_rate;
    /* 0 none, 1 AAC, 2 other. */
    int32_t audio_codec;
    int64_t audio_bit_rate;
};

/* What to make. */
struct piirit_target {
    int32_t width;
    int32_t height;
    int64_t video_bit_rate;
    /* 0 copies sound. */
    int64_t audio_bit_rate;
};

/* Progress in thousandths; non-zero return stops. */
typedef int (*piirit_progress)(void *context, int32_t permille);

/* Results. */
#define PIIRIT_OK 0
#define PIIRIT_CANCELLED 1
#define PIIRIT_FAILED (-1)

int piirit_video_probe(const char *path, struct piirit_probe *out,
                       char *error, size_t error_len);

int piirit_video_recode(const char *input, const char *output,
                        const struct piirit_target *target,
                        piirit_progress progress, void *context,
                        char *error, size_t error_len);

/* Test clip: `seconds` moving H.264, `width` x `height`, `bit_rate`; AAC tone
 * if `with_sound`; `rotation` applied. No camera or binary fixture in tests. */
struct piirit_clip {
    int32_t width;
    int32_t height;
    int32_t seconds;
    int32_t rotation;
    int64_t bit_rate;
    int32_t with_sound;
};

int piirit_video_synth(const char *path, const struct piirit_clip *clip,
                       char *error, size_t error_len);

#endif
