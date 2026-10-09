/*
 * The C side of piirit-video: what Rust calls, in plain types.
 *
 * FFmpeg's and x264's own APIs are structs whose layouts change between
 * releases, so they stay on this side. What crosses to Rust is the few
 * numbers below, which src/lib.rs mirrors field for field.
 */
#ifndef PIIRIT_RECODE_H
#define PIIRIT_RECODE_H

#include <stddef.h>
#include <stdint.h>

/* The video codecs this can decode, and "something else". */
enum piirit_codec {
    PIIRIT_CODEC_NONE = 0,
    PIIRIT_CODEC_H264 = 1,
    PIIRIT_CODEC_HEVC = 2,
    PIIRIT_CODEC_OTHER = 3,
};

/* What a file holds, read off its header without decoding anything. */
struct piirit_probe {
    int64_t duration_ms;
    /* The picture as stored, before the display matrix turns it. */
    int32_t width;
    int32_t height;
    /* How far the display matrix turns it clockwise: 0, 90, 180, 270. */
    int32_t rotation;
    int32_t video_codec;
    int64_t video_bit_rate;
    /* 0 for no sound, 1 for AAC, 2 for anything else. */
    int32_t audio_codec;
    int64_t audio_bit_rate;
};

/* What to make of it. */
struct piirit_target {
    int32_t width;
    int32_t height;
    int64_t video_bit_rate;
    /* 0 copies the sound as it is. */
    int64_t audio_bit_rate;
};

/* Told how far along the recoding is, in thousandths; a non-zero answer
 * stops it. */
typedef int (*piirit_progress)(void *context, int32_t permille);

/* The results. */
#define PIIRIT_OK 0
#define PIIRIT_CANCELLED 1
#define PIIRIT_FAILED (-1)

int piirit_video_probe(const char *path, struct piirit_probe *out,
                       char *error, size_t error_len);

int piirit_video_recode(const char *input, const char *output,
                        const struct piirit_target *target,
                        piirit_progress progress, void *context,
                        char *error, size_t error_len);

/* A test clip to make: `seconds` of moving H.264 at `width` x `height`
 * and `bit_rate`, with a tone in AAC when `with_sound`, turned by
 * `rotation`. For the tests, which have no camera and should not carry
 * a binary fixture. */
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
